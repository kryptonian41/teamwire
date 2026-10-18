//! The start of one member: the `codex` argv, and the token file.
//!
//! Every per-run setting travels as a `-c key=value` override, so nothing is
//! written to `~/.codex/config.toml`. The token goes only into
//! `<member_dir>/token` (mode 0600). Argv holds that file's path, because
//! argv is readable by every process on the machine (ADR 0011).

use std::path::{Path, PathBuf};

use teamwire_core::{
    HarnessError, HarnessErrorCode, IdentityChannel, LaunchFile, LaunchSpec, MemberLaunch,
    SessionId,
};
use toml::{Table, Value};

/// The hook timeout Codex enforces on the Stop hook, in seconds. It is the
/// `hook_timeout_s` default of core's `Config`, kept above the 40 s budget the
/// hook itself works to.
const STOP_HOOK_TIMEOUT_S: i64 = 60;

const PRIVATE: u32 = 0o600;

/// The lead's plugin (ADR 0018), the one plugin id in Rust. Every member
/// switches it off.
const LEAD_PLUGIN_ID: &str = "teamwire@teamwire";

/// Which command starts the session: a fresh one, or a resume of `id`.
pub(crate) enum Start<'a> {
    New,
    Resume(&'a SessionId),
}

pub(crate) fn build(
    codex_bin: &str,
    m: &MemberLaunch,
    start: &Start<'_>,
) -> Result<LaunchSpec, HarnessError> {
    let token = m
        .identity
        .iter()
        .find(|(k, _)| k == "TEAMWIRE_TOKEN")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| {
            HarnessError::new(
                HarnessErrorCode::Refused,
                "the member has no TEAMWIRE_TOKEN, so its identity cannot reach it",
            )
        })?;
    let contract = std::fs::read_to_string(&m.contract_path).map_err(|e| {
        HarnessError::new(
            HarnessErrorCode::Io,
            format!(
                "cannot read the contract {}: {e}",
                m.contract_path.display()
            ),
        )
    })?;
    let token_path = m.member_dir.join("token");
    refuse_own_codex_config(&m.cwd)?;

    let mut argv = vec![codex_bin.to_owned()];
    if let Start::Resume(id) = start {
        argv.extend(["resume".to_owned(), id.as_str().to_owned()]);
    }
    argv.extend(
        [
            "--no-daemon",
            "--dangerously-bypass-approvals-and-sandbox",
            "--dangerously-bypass-hook-trust",
            "-C",
        ]
        .map(str::to_owned),
    );
    argv.push(path_text(&m.cwd));
    if let Some(model) = &m.model {
        argv.extend(["-m".to_owned(), model.clone()]);
    }
    let mut set = |key: &str, value: Value| {
        argv.extend(["-c".to_owned(), format!("{key}={}", inline(&value))]);
    };
    if let Some(effort) = &m.effort {
        set("model_reasoning_effort", text(effort));
    }
    set("developer_instructions", text(&contract));
    set("check_for_update_on_startup", Value::Boolean(false));
    set("tui.disable_paste_burst", Value::Boolean(true));
    if m.accept_trust {
        set(
            "projects",
            table([(
                path_text(&trust_root(&m.cwd)).as_str(),
                table([("trust_level", text("trusted"))]),
            )]),
        );
    }
    let home = home_from_member_dir(&m.member_dir);
    set(
        "shell_environment_policy.set",
        shell_env(m, home.as_ref(), &token_path),
    );
    set(
        "mcp_servers.teamwire",
        mcp_server(m, home.as_ref(), &token_path),
    );
    set(
        "hooks.Stop",
        hook_entry(
            m,
            home.as_ref(),
            &token_path,
            "turn-end",
            Some(STOP_HOOK_TIMEOUT_S),
        ),
    );
    set(
        "hooks.PostToolUse",
        hook_entry(m, home.as_ref(), &token_path, "post-tool", None),
    );
    set(
        "hooks.SessionStart",
        hook_entry(m, home.as_ref(), &token_path, "session-start", None),
    );
    // The lead plugin is always off in a member, whatever the spec says: a
    // member must never load the lead's skill, MCP server or Stop hook
    // (ADR 0018). The spec's other plugins keep their switch.
    let mut plugins = m.plugins.clone();
    plugins.insert(LEAD_PLUGIN_ID.to_owned(), false);
    set(
        "plugins",
        table(
            plugins
                .iter()
                .map(|(id, on)| (id.as_str(), table([("enabled", Value::Boolean(*on))]))),
        ),
    );

    Ok(LaunchSpec {
        argv,
        // The host cannot be relied on to pass environment (ADR 0008), so
        // identity travels in `-c` values and nothing is set here.
        env: Vec::new(),
        files: vec![LaunchFile {
            path: token_path,
            bytes: token.into_bytes(),
            mode: PRIVATE,
        }],
        identity_channel: IdentityChannel::HarnessConfig,
    })
}

/// `value` as TOML on one line. `Value`'s `Display` prints a multi-line string
/// as a `"""` block with raw newlines, and herdr refuses an argument that
/// holds one.
fn inline(value: &Value) -> String {
    match value {
        Value::String(s) => quoted(s),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(inline).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Table(t) if t.is_empty() => "{}".to_owned(),
        Value::Table(t) => {
            let pairs: Vec<String> = t
                .iter()
                .map(|(k, v)| format!("{} = {}", key(k), inline(v)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        }
        // These print on one line already.
        Value::Boolean(_) | Value::Integer(_) | Value::Float(_) | Value::Datetime(_) => {
            value.to_string()
        }
    }
}

/// A basic string with every control character escaped.
fn quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.extend(format!("\\u{:04X}", u32::from(c)).chars()),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn key(k: &str) -> String {
    let bare = !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare { k.to_owned() } else { quoted(k) }
}

fn path_text(p: &Path) -> String {
    p.display().to_string()
}

fn text(s: &str) -> Value {
    Value::String(s.to_owned())
}

fn table<'a>(entries: impl IntoIterator<Item = (&'a str, Value)>) -> Value {
    Value::Table(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<Table>(),
    )
}

/// `$TEAMWIRE_HOME/teams/<team>/members/<role>` names its home four levels up.
fn home_from_member_dir(member_dir: &Path) -> Option<String> {
    let home = member_dir.ancestors().nth(4)?;
    (!home.as_os_str().is_empty()).then(|| path_text(home))
}

/// The folder Codex keys trust on: the main repository's root for a worktree
/// (its `.git` file reads `gitdir: <repo>/.git/worktrees/<name>`), else `cwd`.
fn trust_root(cwd: &Path) -> PathBuf {
    let from_git_file = || {
        let line = std::fs::read_to_string(cwd.join(".git")).ok()?;
        let gitdir = Path::new(line.trim().strip_prefix("gitdir:")?.trim());
        let gitdir = cwd.join(gitdir);
        let dot_git = gitdir
            .ancestors()
            .find(|a| a.file_name().is_some_and(|n| n == ".git"))?;
        Some(dot_git.parent()?.to_path_buf())
    };
    from_git_file().unwrap_or_else(|| cwd.to_path_buf())
}

/// Files that make Codex run what the repository declares. The member's
/// `--dangerously-bypass-hook-trust` runs a hook from either one without
/// review, so teamwire will not start a member in such a folder (the
/// counterpart of INV-25 for Claude members).
const OWN_CONFIG_FILES: [&str; 2] = [".codex/config.toml", ".codex/hooks.json"];

/// Refuses when any folder from the trust root down to `cwd`, inclusive,
/// holds one of `OWN_CONFIG_FILES`.
fn refuse_own_codex_config(cwd: &Path) -> Result<(), HarnessError> {
    let root = trust_root(cwd);
    let mut folders: Vec<&Path> = cwd.ancestors().collect();
    // A root that is not above `cwd` leaves only `cwd` to check.
    folders.truncate(folders.iter().position(|f| *f == root).map_or(1, |i| i + 1));
    for folder in folders {
        for name in OWN_CONFIG_FILES {
            let file = folder.join(name);
            if file.exists() {
                return Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    format!(
                        "{} exists: teamwire will not start a Codex member whose folder declares its own Codex config, because the bypass flag would run it unreviewed",
                        file.display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Quote `text` for `sh -c` only when it holds a character the shell would
/// treat specially, so plain paths stay readable.
fn shell_word(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:=@%,".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

fn shell_env(m: &MemberLaunch, home: Option<&String>, token_path: &Path) -> Value {
    let mut env = vec![
        ("TEAMWIRE_TEAM", text(m.team.as_str())),
        ("TEAMWIRE_ROLE", text(m.role.as_str())),
    ];
    if let Some(h) = home {
        env.push(("TEAMWIRE_HOME", text(h)));
    }
    env.push(("TEAMWIRE_TOKEN_FILE", text(&path_text(token_path))));
    table(env)
}

fn mcp_server(m: &MemberLaunch, home: Option<&String>, token_path: &Path) -> Value {
    let mut args = Vec::new();
    if let Some(h) = home {
        args.extend(["--home", h]);
    }
    args.extend(["mcp", "--team", m.team.as_str(), "--role", m.role.as_str()]);
    table([
        ("command", text(&path_text(&m.teamwire_bin))),
        ("args", Value::Array(args.into_iter().map(text).collect())),
        (
            "env",
            table([("TEAMWIRE_TOKEN_FILE", text(&path_text(token_path)))]),
        ),
    ])
}

/// One hook, as one `sh` command line. Codex gives the hook its own
/// environment, which carries no teamwire variable under herdr, so the line
/// sets them.
fn hook_entry(
    m: &MemberLaunch,
    home: Option<&String>,
    token_path: &Path,
    event: &str,
    timeout_s: Option<i64>,
) -> Value {
    let mut words = vec![
        format!("TEAMWIRE_TEAM={}", shell_word(m.team.as_str())),
        format!("TEAMWIRE_ROLE={}", shell_word(m.role.as_str())),
    ];
    if let Some(h) = home {
        words.push(format!("TEAMWIRE_HOME={}", shell_word(h)));
    }
    words.push(format!(
        "TEAMWIRE_TOKEN_FILE={}",
        shell_word(&path_text(token_path))
    ));
    words.push(shell_word(&path_text(&m.teamwire_bin)));
    if let Some(h) = home {
        words.extend(["--home".to_owned(), shell_word(h)]);
    }
    words.extend([
        "hook".to_owned(),
        event.to_owned(),
        "--team".to_owned(),
        shell_word(m.team.as_str()),
        "--role".to_owned(),
        shell_word(m.role.as_str()),
    ]);
    let mut hook = vec![
        ("type", text("command")),
        ("command", text(&words.join(" "))),
    ];
    if let Some(t) = timeout_s {
        hook.push(("timeout", Value::Integer(t)));
    }
    Value::Array(vec![table([("hooks", Value::Array(vec![table(hook)]))])])
}
