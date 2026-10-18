//! The only code that names concrete adapters: it fills the `Registry` with
//! every adapter this binary has and picks the lead's harness and host from
//! the environment.

use std::path::PathBuf;
use std::sync::Arc;

use teamwire_core::{HarnessId, HostId, NoHost, Platform, Registry, RegistryError};
use teamwire_harness_claude::ClaudeCode;
use teamwire_harness_codex::{CodexCli, CodexConfig};
use teamwire_host_herdr::Herdr;
use teamwire_host_tmux::Tmux;
use teamwire_platform::SystemClock;

// The Claude Code lead-setup items that `install`, `doctor` and the hook
// name. They are re-exported here so no other file names an adapter crate.
pub(crate) use teamwire_harness_claude::ClaudeCode as LeadSetup;
pub(crate) use teamwire_harness_claude::{
    ApplyError, Change, ClaudeConfig, McpEntry, McpState, SKILL_TEXT, SkillState, edited_skill,
    mcp_state, shadowing_entries, skill_state,
};

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
    /// Where Claude Code keeps its files, for the commands that read or
    /// write the lead setup.
    pub(crate) claude: ClaudeConfig,
}

impl Wiring {
    /// The Claude Code adapter that reads and writes the lead setup.
    pub(crate) fn lead_setup(&self) -> LeadSetup {
        LeadSetup::new(Arc::clone(&self.platform), self.claude.clone())
    }
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
    /// `TEAMWIRE_LEAD_HARNESS`: the lead's harness id, when the user names it.
    pub(crate) lead_harness: Option<String>,
    /// `CLAUDE_CODE_SESSION_ID`: set inside a Claude Code session.
    pub(crate) claude_session: Option<String>,
    /// `CODEX_THREAD_ID`: set inside a Codex session.
    pub(crate) codex_thread: Option<String>,
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
            lead_harness: std::env::var("TEAMWIRE_LEAD_HARNESS").ok(),
            claude_session: std::env::var("CLAUDE_CODE_SESSION_ID").ok(),
            codex_thread: std::env::var("CODEX_THREAD_ID").ok(),
        }
    }
}

pub(crate) fn wire(env: &WiringEnv) -> Result<Wiring, RegistryError> {
    let platform = teamwire_platform::native();
    let claude = claude_config(env);
    let mut registry = Registry::new();
    registry.add_harness(Arc::new(ClaudeCode::new(
        Arc::clone(&platform),
        claude.clone(),
    )))?;
    registry.add_harness(Arc::new(CodexCli::new(
        Arc::clone(&platform),
        codex_config(env),
    )))?;
    registry.add_host(Arc::new(Herdr::new(Arc::clone(&platform))))?;
    registry.add_host(Arc::new(tmux_host(Arc::clone(&platform), env)))?;
    registry.add_host(Arc::new(NoHost))?;
    Ok(Wiring {
        registry,
        platform,
        clock: SystemClock,
        lead_harness: lead_harness(env),
        lead_host: lead_host(env),
        claude,
    })
}

/// `TEAMWIRE_LEAD_HARNESS` when it names a harness of this binary, else the
/// harness whose session the command runs in: `claude-code` when
/// `CLAUDE_CODE_SESSION_ID` is set, `codex` when `CODEX_THREAD_ID` is set,
/// and `claude-code` otherwise. An id this binary lacks is ignored, so a typo
/// cannot leave the lead without an adapter.
fn lead_harness(env: &WiringEnv) -> HarnessId {
    let set = |v: &Option<String>| v.as_deref().is_some_and(|v| !v.is_empty());
    let id = match env.lead_harness.as_deref() {
        Some(CodexCli::ID) => CodexCli::ID,
        Some(ClaudeCode::ID) => ClaudeCode::ID,
        _ if set(&env.claude_session) => ClaudeCode::ID,
        _ if set(&env.codex_thread) => CodexCli::ID,
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

/// `herdr` inside a herdr pane (`HERDR_ENV=1`), else `tmux` inside a tmux
/// client (`TMUX` set), otherwise `none`. herdr wins when both are set,
/// because a herdr pane can itself be started from a tmux session.
fn lead_host(env: &WiringEnv) -> HostId {
    if env.herdr_env.as_deref() == Some("1") {
        HostId::from_static(Herdr::ID)
    } else if env.tmux.as_deref().is_some_and(|t| !t.is_empty()) {
        HostId::from_static(Tmux::ID)
    } else {
        HostId::from_static(NoHost::ID)
    }
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
            lead_harness: None,
            claude_session: None,
            codex_thread: None,
        }
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
        assert_eq!(lead_with(None, Some(""), Some("t1")), "codex");
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
            lead_of(&env_in_tmux(Some("1"), Some("/tmp/tmux-1/default,42,0"))),
            "herdr"
        );
        assert_eq!(lead_of(&env_in_tmux(None, Some(""))), "none");
        assert_eq!(lead_of(&env_in_tmux(None, None)), "none");
    }

    #[test]
    fn registry_holds_every_adapter_of_this_binary() {
        let wiring = wire(&env(None)).unwrap();
        assert_eq!(wiring.registry.harness_ids(), ["claude-code", "codex"]);
        assert_eq!(wiring.registry.host_ids(), ["herdr", "none", "tmux"]);
        assert!(wiring.registry.harness(&wiring.lead_harness).is_ok());
        assert!(wiring.registry.host(&wiring.lead_host).is_ok());
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
