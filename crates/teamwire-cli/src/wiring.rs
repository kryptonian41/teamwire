//! The only code that names concrete adapters: it fills the `Registry` with
//! every adapter this binary has and picks the lead's harness and host from
//! the environment.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use teamwire_core::{HarnessId, HostId, NoHost, Pid, Platform, Registry, RegistryError};

use crate::cmd::doctor::pick_host;
use teamwire_harness_claude::ClaudeCode;
use teamwire_harness_codex::{CodexCli, CodexConfig};
use teamwire_harness_pi::Pi;
use teamwire_host_herdr::Herdr;
use teamwire_host_tmux::Tmux;
use teamwire_platform::SystemClock;

// What `install` and `doctor` name of the Claude Code and pi adapters: the
// shared skill every lead setup is given (ADR 0023 point 9), and the folders
// doctor reads herdr's integrations from. Re-exported here so no other file
// names an adapter crate; each lead setup itself is reached through the
// `Registry` (ADR 0031).
pub(crate) use teamwire_harness_claude::{ClaudeConfig, SKILL_TEXT};
pub(crate) use teamwire_harness_pi::PiConfig;

/// The harness whose hook commands this binary serves.
pub(crate) fn claude_harness_id() -> HarnessId {
    HarnessId::from_static(ClaudeCode::ID)
}

/// The adapters and the platform one command runs with.
pub(crate) struct Wiring {
    pub(crate) registry: Registry,
    pub(crate) platform: Arc<dyn Platform>,
    pub(crate) clock: SystemClock,
    pub(crate) lead_harness: HarnessId,
    pub(crate) lead_host: HostId,
    /// Where Claude Code keeps its files: doctor reads herdr's session hook
    /// from its `settings.json`.
    pub(crate) claude: ClaudeConfig,
    /// Where pi keeps its files and how to start it: doctor reads herdr's
    /// pi integration from the directory.
    pub(crate) pi: PiConfig,
}

/// The environment facts wiring reads, gathered in one place so tests can
/// pass their own.
pub(crate) struct WiringEnv {
    /// `HERDR_ENV`: `1` inside a herdr pane.
    pub(crate) herdr_env: Option<String>,
    /// `TMUX`: set inside a tmux client, to the server's socket path.
    pub(crate) tmux: Option<String>,
    /// `HOME`.
    pub(crate) user_home: Option<PathBuf>,
    /// `TEAMWIRE_CLAUDE_DIR`: where tests put Claude's files.
    pub(crate) claude_dir: Option<PathBuf>,
    /// `CLAUDE_CONFIG_DIR`: Claude Code's own setting for the same thing.
    pub(crate) claude_config_dir: Option<PathBuf>,
    /// `TEAMWIRE_CODEX_HOME`: where tests put Codex's files.
    pub(crate) codex_test_home: Option<PathBuf>,
    /// `CODEX_HOME`: Codex's own setting for the same thing.
    pub(crate) codex_home: Option<PathBuf>,
    /// `PI_CODING_AGENT_DIR`: pi's own setting for the same thing.
    pub(crate) pi_agent_dir: Option<PathBuf>,
    /// `TEAMWIRE_LEAD_HARNESS`: the lead's harness id, when the user names it.
    pub(crate) lead_harness: Option<String>,
    /// `CLAUDE_CODE_SESSION_ID`: set inside a Claude Code session.
    pub(crate) claude_session: Option<String>,
    /// `CODEX_THREAD_ID`: set inside a Codex session.
    pub(crate) codex_thread: Option<String>,
    /// `PI_CODING_AGENT`: `true` in pi and in every process pi starts.
    pub(crate) pi_marker: Option<String>,
}

impl WiringEnv {
    pub(crate) fn from_process() -> Self {
        Self {
            herdr_env: std::env::var("HERDR_ENV").ok(),
            tmux: std::env::var("TMUX").ok(),
            user_home: std::env::var_os("HOME").map(PathBuf::from),
            claude_dir: std::env::var_os("TEAMWIRE_CLAUDE_DIR").map(PathBuf::from),
            claude_config_dir: std::env::var_os("CLAUDE_CONFIG_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            codex_test_home: std::env::var_os("TEAMWIRE_CODEX_HOME").map(PathBuf::from),
            codex_home: std::env::var_os("CODEX_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            pi_agent_dir: std::env::var_os("PI_CODING_AGENT_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            lead_harness: std::env::var("TEAMWIRE_LEAD_HARNESS").ok(),
            claude_session: std::env::var("CLAUDE_CODE_SESSION_ID").ok(),
            codex_thread: std::env::var("CODEX_THREAD_ID").ok(),
            pi_marker: std::env::var("PI_CODING_AGENT").ok(),
        }
    }
}

pub(crate) fn wire(env: &WiringEnv) -> Result<Wiring, RegistryError> {
    wire_with_home(env, None)
}

/// `wire` for a command run with the global `--home` flag. The flag wins
/// over `TEAMWIRE_HOME` wherever wiring reads the state directory, as it
/// does for every command.
pub(crate) fn wire_with_home(
    env: &WiringEnv,
    home_flag: Option<&Path>,
) -> Result<Wiring, RegistryError> {
    let platform = teamwire_platform::native();
    let claude = claude_config(env);
    let codex = codex_config(env);
    let pi = pi_config(env, home_flag);
    let mut registry = Registry::new();
    registry.add_harness(Arc::new(ClaudeCode::new(
        Arc::clone(&platform),
        claude.clone(),
    )))?;
    registry.add_harness(Arc::new(CodexCli::new(Arc::clone(&platform), codex)))?;
    registry.add_harness(Arc::new(Pi::new(Arc::clone(&platform), pi.clone())))?;
    registry.add_host(Arc::new(Herdr::new(Arc::clone(&platform))))?;
    registry.add_host(Arc::new(tmux_host(Arc::clone(&platform), env)))?;
    registry.add_host(Arc::new(NoHost))?;
    let lead_host = lead_host(env, &|pid| platform.parent_of(pid));
    Ok(Wiring {
        registry,
        platform,
        clock: SystemClock,
        lead_harness: lead_harness(env),
        lead_host,
        claude,
        pi,
    })
}

/// `TEAMWIRE_LEAD_HARNESS` when it names a harness of this binary, else the
/// harness whose session the command runs in: `claude-code` when
/// `CLAUDE_CODE_SESSION_ID` is set, `codex` when `CODEX_THREAD_ID` is set,
/// `pi` when `PI_CODING_AGENT` is `true`, and `claude-code` otherwise. pi's
/// marker comes last because every process pi starts inherits it, so a
/// Claude or Codex session started from a pi shell is still itself. An id
/// this binary lacks is ignored, so a typo cannot leave the lead without an
/// adapter.
fn lead_harness(env: &WiringEnv) -> HarnessId {
    let set = |v: &Option<String>| v.as_deref().is_some_and(|v| !v.is_empty());
    let id = match env.lead_harness.as_deref() {
        Some(CodexCli::ID) => CodexCli::ID,
        Some(ClaudeCode::ID) => ClaudeCode::ID,
        Some(Pi::ID) => Pi::ID,
        _ if set(&env.claude_session) => ClaudeCode::ID,
        _ if set(&env.codex_thread) => CodexCli::ID,
        _ if env.pi_marker.as_deref() == Some("true") => Pi::ID,
        _ => ClaudeCode::ID,
    };
    HarnessId::from_static(id)
}

/// `TEAMWIRE_CODEX_HOME`, else `CODEX_HOME`, else `~/.codex`.
fn codex_config(env: &WiringEnv) -> CodexConfig {
    let codex_home = match (
        env.codex_test_home.as_ref().or(env.codex_home.as_ref()),
        &env.user_home,
    ) {
        (Some(dir), _) => dir.clone(),
        (None, Some(home)) => home.join(".codex"),
        (None, None) => PathBuf::from(".codex"),
    };
    CodexConfig {
        codex_home,
        codex_bin: "codex".to_owned(),
    }
}

/// `PI_CODING_AGENT_DIR`, else `~/.pi/agent`. The providers to leave out
/// come from `pi_exclude_providers`, which reads `home_flag`'s config file.
fn pi_config(env: &WiringEnv, home_flag: Option<&Path>) -> PiConfig {
    let agent_dir = match (&env.pi_agent_dir, &env.user_home) {
        (Some(dir), _) => dir.clone(),
        (None, Some(home)) => home.join(".pi").join("agent"),
        (None, None) => PathBuf::from(".pi/agent"),
    };
    PiConfig {
        agent_dir,
        pi_bin: "pi".to_owned(),
        exclude_providers: pi_exclude_providers(home_flag, env.user_home.as_deref(), &|key| {
            std::env::var(key).ok()
        }),
    }
}

/// The key that names the pi providers teamwire leaves out of its model list.
const PI_EXCLUDE_KEY: &str = "TEAMWIRE_PI_EXCLUDE_PROVIDERS";

/// `TEAMWIRE_PI_EXCLUDE_PROVIDERS` in the environment, else the same key as
/// a `KEY = "value"` line in `$TEAMWIRE_HOME/config.toml`, the way the OTLP
/// settings are read. `env` looks up one variable and an empty value counts
/// as unset. The value is a comma-separated list; spaces around a name and
/// empty names are dropped. The state directory is `home_flag` (`--home`),
/// else `TEAMWIRE_HOME`, else `~/.teamwire`, the order every command uses.
fn pi_exclude_providers(
    home_flag: Option<&Path>,
    user_home: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let value = env(PI_EXCLUDE_KEY).filter(|v| !v.is_empty()).or_else(|| {
        let home = home_flag
            .map(Path::to_path_buf)
            .or_else(|| {
                env("TEAMWIRE_HOME")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(|| user_home.map(|h| h.join(".teamwire")))?;
        let file = std::fs::read_to_string(home.join("config.toml")).ok()?;
        file_setting(&file, PI_EXCLUDE_KEY)
    });
    value
        .iter()
        .flat_map(|list| list.split(','))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The value of `KEY = value` on one line of `config.toml`, in double quotes,
/// single quotes or bare. A `#` after a bare value starts a comment.
fn file_setting(file: &str, key: &str) -> Option<String> {
    file.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let v = v.trim();
        let value = match v.chars().next()? {
            quote @ ('"' | '\'') => v.get(1..)?.split_once(quote)?.0,
            _ => v.split_once(" #").map_or(v, |(bare, _)| bare).trim(),
        };
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// The tmux adapter. Inside a tmux client it takes that client's server for
/// every place that records none (an older team, or `TMUX_PANE` from a
/// process); a place a launch recorded names its own server, so a command
/// from outside tmux still reaches the team.
fn tmux_host(platform: Arc<dyn Platform>, env: &WiringEnv) -> Tmux {
    let tmux = Tmux::new(platform);
    match env.tmux.as_deref() {
        Some(client) => tmux.with_client(client),
        None => tmux,
    }
}

fn lead_host(env: &WiringEnv, parent_of: &dyn Fn(Pid) -> Option<(Pid, String)>) -> HostId {
    HostId::from_static(pick_host(env.herdr_env.as_deref(), env.tmux.as_deref(), parent_of).id)
}

/// Claude's files: `TEAMWIRE_CLAUDE_DIR`, else `CLAUDE_CONFIG_DIR`, holds
/// both Claude's directory and `.claude.json` when set, as Claude Code lays
/// out `CLAUDE_CONFIG_DIR`; otherwise `~/.claude` and `~/.claude.json`.
fn claude_config(env: &WiringEnv) -> ClaudeConfig {
    let set_dir = env.claude_dir.as_ref().or(env.claude_config_dir.as_ref());
    let (claude_dir, claude_json) = match (set_dir, &env.user_home) {
        (Some(dir), _) => (dir.clone(), dir.join(".claude.json")),
        (None, Some(home)) => (home.join(".claude"), home.join(".claude.json")),
        (None, None) => (PathBuf::from(".claude"), PathBuf::from(".claude.json")),
    };
    ClaudeConfig {
        claude_dir,
        claude_json,
        claude_bin: "claude".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(herdr: Option<&str>) -> WiringEnv {
        WiringEnv {
            herdr_env: herdr.map(str::to_owned),
            tmux: None,
            user_home: Some(PathBuf::from("/home/u")),
            claude_dir: None,
            claude_config_dir: None,
            codex_test_home: None,
            codex_home: None,
            pi_agent_dir: None,
            lead_harness: None,
            claude_session: None,
            codex_thread: None,
            pi_marker: None,
        }
    }

    fn lead_in_pi(var: Option<&str>, claude: Option<&str>) -> String {
        let e = WiringEnv {
            lead_harness: var.map(str::to_owned),
            claude_session: claude.map(str::to_owned),
            pi_marker: Some("true".to_owned()),
            ..env(None)
        };
        wire(&e).unwrap().lead_harness.as_str().to_owned()
    }

    fn lead_with(var: Option<&str>, claude: Option<&str>, codex: Option<&str>) -> String {
        let e = WiringEnv {
            lead_harness: var.map(str::to_owned),
            claude_session: claude.map(str::to_owned),
            codex_thread: codex.map(str::to_owned),
            ..env(None)
        };
        wire(&e).unwrap().lead_harness.as_str().to_owned()
    }

    #[test]
    fn the_lead_harness_follows_the_variable_then_the_session() {
        assert_eq!(lead_with(None, None, None), "claude-code");
        assert_eq!(lead_with(None, None, Some("t1")), "codex");
        assert_eq!(lead_with(None, Some("s1"), Some("t1")), "claude-code");
        assert_eq!(lead_with(Some("codex"), Some("s1"), None), "codex");
        assert_eq!(
            lead_with(Some("claude-code"), None, Some("t1")),
            "claude-code"
        );
        // An empty or unknown value does not decide.
        assert_eq!(lead_with(Some("nope"), None, Some("t1")), "codex");
        assert_eq!(lead_with(Some("pi"), None, None), "pi");
        assert_eq!(lead_with(None, Some(""), Some("t1")), "codex");
    }

    #[test]
    fn pis_marker_picks_pi_after_every_other_session() {
        assert_eq!(lead_in_pi(None, None), "pi");
        // A Claude session started from a pi shell carries both.
        assert_eq!(lead_in_pi(None, Some("s1")), "claude-code");
        assert_eq!(lead_in_pi(Some("codex"), None), "codex");
        let e = WiringEnv {
            pi_marker: Some("false".to_owned()),
            ..env(None)
        };
        assert_eq!(wire(&e).unwrap().lead_harness.as_str(), "claude-code");
    }

    #[test]
    fn herdr_env_one_picks_herdr_for_the_lead() {
        assert_eq!(wire(&env(Some("1"))).unwrap().lead_host.as_str(), "herdr");
        assert_eq!(wire(&env(None)).unwrap().lead_host.as_str(), "none");
        assert_eq!(wire(&env(Some("0"))).unwrap().lead_host.as_str(), "none");
    }

    fn env_in_tmux(herdr: Option<&str>, tmux: Option<&str>) -> WiringEnv {
        WiringEnv {
            tmux: tmux.map(str::to_owned),
            ..env(herdr)
        }
    }

    fn lead_of(env: &WiringEnv) -> String {
        wire(env).unwrap().lead_host.as_str().to_owned()
    }

    #[test]
    fn tmux_set_and_herdr_not_picks_tmux_for_the_lead() {
        assert_eq!(
            lead_of(&env_in_tmux(None, Some("/tmp/tmux-1/default,42,0"))),
            "tmux"
        );
        assert_eq!(
            lead_of(&env_in_tmux(Some("0"), Some("/tmp/tmux-1/default,42,0"))),
            "tmux"
        );
    }

    #[test]
    fn the_tmux_adapter_asks_the_server_the_tmux_variable_names() {
        use teamwire_core::{Host, Place};
        use teamwire_fakes::FakePlatform;
        let asked = |tmux: Option<&str>| {
            let platform = Arc::new(FakePlatform::new());
            let host = tmux_host(platform.clone(), &env_in_tmux(None, tmux));
            drop(host.state(
                &Place {
                    host: HostId::from_static("tmux"),
                    id: "%1".to_owned(),
                },
                std::time::Duration::from_secs(1),
            ));
            platform.runs().first().unwrap().0.args.clone()
        };
        assert_eq!(
            asked(Some("/tmp/tmux-1/team,42,0")).first_chunk::<2>(),
            Some(&["-S".to_owned(), "/tmp/tmux-1/team".to_owned()])
        );
        assert_ne!(
            asked(None).first().map(String::as_str),
            Some("-S"),
            "outside tmux there is no server to name"
        );
    }

    #[test]
    fn herdr_wins_over_tmux_and_an_empty_tmux_is_not_tmux() {
        assert_eq!(
            lead_host(
                &env_in_tmux(Some("1"), Some("/tmp/tmux-1/default,42,0")),
                &|_| None
            )
            .as_str(),
            "herdr"
        );
        assert_eq!(lead_of(&env_in_tmux(None, Some(""))), "none");
        assert_eq!(lead_of(&env_in_tmux(None, None)), "none");
    }

    #[test]
    fn registry_holds_every_adapter_of_this_binary() {
        let wiring = wire(&env(None)).unwrap();
        assert_eq!(
            wiring.registry.harness_ids(),
            ["claude-code", "codex", "pi"]
        );
        assert_eq!(wiring.registry.host_ids(), ["herdr", "none", "tmux"]);
        assert!(wiring.registry.harness(&wiring.lead_harness).is_ok());
        assert!(wiring.registry.host(&wiring.lead_host).is_ok());
    }

    #[test]
    fn pis_directory_is_pis_own_setting_then_the_default() {
        let mut e = env(None);
        assert_eq!(
            pi_config(&e, None).agent_dir,
            PathBuf::from("/home/u/.pi/agent")
        );
        assert_eq!(
            pi_config(&e, None).sessions_dir(),
            PathBuf::from("/home/u/.pi/agent/sessions/teamwire")
        );
        e.pi_agent_dir = Some(PathBuf::from("/pi"));
        assert_eq!(pi_config(&e, None).agent_dir, PathBuf::from("/pi"));
    }

    fn write(path: &Path, text: &str) {
        teamwire_platform::native()
            .write_atomic(path, text.as_bytes())
            .unwrap();
    }

    fn excluded(
        env: &[(&str, &str)],
        config: Option<&str>,
        user_home: Option<&Path>,
    ) -> Vec<String> {
        let home = tempfile::tempdir().unwrap();
        if let Some(config) = config {
            write(&home.path().join("config.toml"), config);
        }
        pi_exclude_providers(None, user_home, &|key| {
            env.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
                .or_else(|| {
                    (key == "TEAMWIRE_HOME" && user_home.is_none())
                        .then(|| home.path().display().to_string())
                })
        })
    }

    #[test]
    fn excluded_providers_come_from_the_environment_split_on_commas() {
        let list = [(
            "TEAMWIRE_PI_EXCLUDE_PROVIDERS",
            " amazon-bedrock, google ,,x",
        )];
        assert_eq!(
            excluded(&list, None, None),
            ["amazon-bedrock", "google", "x"]
        );
        assert!(excluded(&[], None, None).is_empty());
        let empty = [("TEAMWIRE_PI_EXCLUDE_PROVIDERS", "")];
        assert!(excluded(&empty, None, None).is_empty());
    }

    #[test]
    fn the_environment_wins_over_the_file_and_the_file_fills_in_without_it() {
        let file = "TEAMWIRE_PI_EXCLUDE_PROVIDERS = \"amazon-bedrock, azure\"\n";
        let env = [("TEAMWIRE_PI_EXCLUDE_PROVIDERS", "google")];
        assert_eq!(excluded(&env, Some(file), None), ["google"]);
        assert_eq!(excluded(&[], Some(file), None), ["amazon-bedrock", "azure"]);
        let empty = [("TEAMWIRE_PI_EXCLUDE_PROVIDERS", "")];
        assert_eq!(
            excluded(&empty, Some(file), None),
            ["amazon-bedrock", "azure"]
        );
    }

    #[test]
    fn the_file_value_may_be_single_quoted_bare_or_followed_by_a_comment() {
        let read = |line: &str| excluded(&[], Some(line), None);
        assert_eq!(read("TEAMWIRE_PI_EXCLUDE_PROVIDERS = 'a,b'"), ["a", "b"]);
        assert_eq!(
            read("TEAMWIRE_PI_EXCLUDE_PROVIDERS = a,b # note"),
            ["a", "b"]
        );
        assert_eq!(
            read("OTHER = \"x\"\nTEAMWIRE_PI_EXCLUDE_PROVIDERS=\"c\""),
            ["c"]
        );
        assert!(read("OTHER = \"x\"").is_empty());
        assert!(read("TEAMWIRE_PI_EXCLUDE_PROVIDERS = \"\"").is_empty());
    }

    #[test]
    fn the_file_is_read_from_the_default_home_when_teamwire_home_is_unset() {
        let user = tempfile::tempdir().unwrap();
        let state = user.path().join(".teamwire");
        std::fs::create_dir_all(&state).unwrap();
        write(
            &state.join("config.toml"),
            "TEAMWIRE_PI_EXCLUDE_PROVIDERS = \"p\"",
        );
        assert_eq!(
            pi_exclude_providers(None, Some(user.path()), &|_| None),
            ["p"]
        );
        assert!(pi_exclude_providers(None, None, &|_| None).is_empty());
    }

    #[test]
    fn the_home_flag_wins_over_teamwire_home_for_the_file() {
        let flag = tempfile::tempdir().unwrap();
        write(
            &flag.path().join("config.toml"),
            "TEAMWIRE_PI_EXCLUDE_PROVIDERS = \"from-flag\"",
        );
        let other = tempfile::tempdir().unwrap();
        write(
            &other.path().join("config.toml"),
            "TEAMWIRE_PI_EXCLUDE_PROVIDERS = \"from-variable\"",
        );
        let variable = other.path().display().to_string();
        let home_set = |key: &str| (key == "TEAMWIRE_HOME").then(|| variable.clone());
        assert_eq!(
            pi_exclude_providers(Some(flag.path()), None, &home_set),
            ["from-flag"]
        );
        assert_eq!(
            pi_exclude_providers(None, None, &home_set),
            ["from-variable"]
        );
        // The environment's list still wins over the flag's file.
        let listed = |key: &str| (key == PI_EXCLUDE_KEY).then(|| "env".to_owned());
        assert_eq!(
            pi_exclude_providers(Some(flag.path()), None, &listed),
            ["env"]
        );
    }

    #[test]
    fn claude_dir_override_holds_both_paths() {
        let mut e = env(None);
        assert_eq!(
            claude_config(&e).claude_json,
            PathBuf::from("/home/u/.claude.json")
        );
        e.claude_dir = Some(PathBuf::from("/tmp/c"));
        let c = claude_config(&e);
        assert_eq!(c.claude_dir, PathBuf::from("/tmp/c"));
        assert_eq!(c.claude_json, PathBuf::from("/tmp/c/.claude.json"));
    }

    #[test]
    fn claude_config_dir_is_followed_and_the_test_override_wins_over_it() {
        let mut e = env(None);
        e.claude_config_dir = Some(PathBuf::from("/cfg"));
        let c = claude_config(&e);
        assert_eq!(c.claude_dir, PathBuf::from("/cfg"));
        assert_eq!(c.claude_json, PathBuf::from("/cfg/.claude.json"));
        e.claude_dir = Some(PathBuf::from("/tmp/c"));
        assert_eq!(claude_config(&e).claude_dir, PathBuf::from("/tmp/c"));
    }
}
