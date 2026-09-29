//! The start of one member: the `claude` argv, and the two per-run files
//! (`settings.json`, `mcp.json`) that carry hooks, MCP server and identity.
//!
//! Launch writes the files and the contract itself; this module only builds
//! their bytes. Every file goes under `member_dir` with mode 0600, because
//! both hold the member's token.

use std::path::Path;

use serde_json::{Map, Value, json};
use teamwire_core::{
    HarnessError, HarnessErrorCode, IdentityChannel, LaunchFile, LaunchSpec, MemberLaunch,
    OtelConfig, Role, SessionId, TeamName,
};

/// The hook timeout Claude Code enforces on the Stop hook, in seconds. It is
/// the `hook_timeout_s` default of core's `Config`, kept above the 40 s
/// budget the hook itself works to.
const STOP_HOOK_TIMEOUT_S: u64 = 60;

const PRIVATE: u32 = 0o600;

/// Which flag names the session: a fresh id, or one to resume.
pub(crate) enum SessionArg<'a> {
    New,
    Resume(&'a SessionId),
}

pub(crate) fn build(
    claude_bin: &str,
    m: &MemberLaunch,
    session: &SessionArg<'_>,
) -> Result<LaunchSpec, HarnessError> {
    let settings_path = m.member_dir.join("settings.json");
    let mcp_path = m.member_dir.join("mcp.json");

    let mut argv = vec![
        claude_bin.to_owned(),
        "-n".to_owned(),
        m.address.as_str().to_owned(),
    ];
    let (flag, id) = match session {
        SessionArg::New => ("--session-id", &m.session_id),
        SessionArg::Resume(id) => ("--resume", *id),
    };
    argv.extend([flag.to_owned(), id.as_str().to_owned()]);
    argv.extend([
        "--permission-mode".to_owned(),
        "bypassPermissions".to_owned(),
        "--append-system-prompt-file".to_owned(),
        path_text(&m.contract_path),
        "--settings".to_owned(),
        path_text(&settings_path),
        "--mcp-config".to_owned(),
        path_text(&mcp_path),
    ]);
    if let Some(model) = &m.model {
        argv.extend(["--model".to_owned(), model.clone()]);
    }
    if let Some(effort) = &m.effort {
        argv.extend(["--effort".to_owned(), effort.clone()]);
    }

    Ok(LaunchSpec {
        argv,
        // The host cannot be relied on to pass environment (ADR 0008), so
        // identity travels in `settings.json` and nothing is set here.
        env: Vec::new(),
        files: vec![
            LaunchFile {
                path: settings_path,
                bytes: to_bytes(&settings_json(m)?)?,
                mode: PRIVATE,
            },
            LaunchFile {
                path: mcp_path,
                bytes: to_bytes(&mcp_json(m)?)?,
                mode: PRIVATE,
            },
        ],
        identity_channel: IdentityChannel::HarnessConfig,
    })
}

fn path_text(p: &Path) -> String {
    p.display().to_string()
}

fn to_bytes(doc: &Value) -> Result<Vec<u8>, HarnessError> {
    serde_json::to_vec_pretty(doc)
        .map_err(|e| HarnessError::new(HarnessErrorCode::Other, e.to_string()))
}

fn identity_value<'a>(m: &'a MemberLaunch, name: &str) -> Option<&'a str> {
    m.identity
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// `$TEAMWIRE_HOME/teams/<team>/members/<role>` names its home four levels up.
fn home_from_member_dir(member_dir: &Path) -> Option<String> {
    let home = member_dir.ancestors().nth(4)?;
    (!home.as_os_str().is_empty()).then(|| path_text(home))
}

/// Quote `text` for `sh -c` only when it holds a character the shell would
/// treat specially, so plain paths stay readable in the file.
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

fn env_object(m: &MemberLaunch) -> Result<Map<String, Value>, HarnessError> {
    let mut env: Vec<(String, String)> = m.identity.clone();
    let mut ensure = |name: &str, value: Option<String>| {
        if !env.iter().any(|(k, _)| k == name)
            && let Some(v) = value
        {
            env.push((name.to_owned(), v));
        }
    };
    ensure("TEAMWIRE_TEAM", Some(m.team.as_str().to_owned()));
    ensure("TEAMWIRE_ROLE", Some(m.role.as_str().to_owned()));
    ensure("TEAMWIRE_HOME", home_from_member_dir(&m.member_dir));
    if !env.iter().any(|(k, _)| k == "TEAMWIRE_TOKEN") {
        return Err(HarnessError::new(
            HarnessErrorCode::Refused,
            "the member has no TEAMWIRE_TOKEN, so its identity cannot reach it",
        ));
    }
    // The identity's own attributes, if launch passed any, come first.
    let existing = identity_value(m, "OTEL_RESOURCE_ATTRIBUTES").map(str::to_owned);
    env.retain(|(k, _)| k != "OTEL_RESOURCE_ATTRIBUTES");
    env.extend(otel_vars(&m.team, &m.role, &m.otel, existing.as_deref()));

    Ok(env
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect())
}

fn hook_entry(m: &MemberLaunch, event: &str, timeout_s: Option<u64>) -> Value {
    let command = format!(
        "{} hook {event} --team {} --role {}",
        shell_word(&path_text(&m.teamwire_bin)),
        m.team.as_str(),
        m.role.as_str()
    );
    let mut hook = json!({ "type": "command", "command": command });
    if let (Some(t), Some(map)) = (timeout_s, hook.as_object_mut()) {
        map.insert("timeout".to_owned(), json!(t));
    }
    json!([{ "hooks": [hook] }])
}

fn settings_json(m: &MemberLaunch) -> Result<Value, HarnessError> {
    Ok(json!({
        "env": Value::Object(env_object(m)?),
        "hooks": {
            "Stop": hook_entry(m, "claude-stop", Some(STOP_HOOK_TIMEOUT_S)),
            "PostToolUse": hook_entry(m, "claude-post-tool", None),
        },
    }))
}

fn mcp_json(m: &MemberLaunch) -> Result<Value, HarnessError> {
    let token = identity_value(m, "TEAMWIRE_TOKEN").ok_or_else(|| {
        HarnessError::new(
            HarnessErrorCode::Refused,
            "the member has no TEAMWIRE_TOKEN",
        )
    })?;
    // The token and the home are in argv because whether a stdio MCP server
    // inherits the session's environment is untested (ADR 0008).
    let home = identity_value(m, "TEAMWIRE_HOME")
        .map(str::to_owned)
        .or_else(|| home_from_member_dir(&m.member_dir));
    let mut args: Vec<&str> = Vec::new();
    if let Some(home) = &home {
        args.extend(["--home", home]);
    }
    args.extend([
        "mcp",
        "--team",
        m.team.as_str(),
        "--role",
        m.role.as_str(),
        "--token",
        token,
    ]);
    Ok(json!({
        "mcpServers": {
            "teamwire": {
                "type": "stdio",
                "command": path_text(&m.teamwire_bin),
                "args": args,
            }
        }
    }))
}

/// A header value goes in a comma-separated list, so `%` and `,` are escaped
/// as the OTLP exporter specification asks.
fn header_value(value: &str) -> String {
    value.replace('%', "%25").replace(',', "%2C")
}

pub(crate) fn otel_vars(
    team: &TeamName,
    role: &Role,
    base: &OtelConfig,
    existing_attributes: Option<&str>,
) -> Vec<(String, String)> {
    let ours = format!(
        "teamwire.team={},teamwire.role={}",
        team.as_str(),
        role.as_str()
    );
    let attributes = match existing_attributes.filter(|a| !a.is_empty()) {
        Some(old) => format!("{old},{ours}"),
        None => ours,
    };
    let mut vars = vec![("OTEL_RESOURCE_ATTRIBUTES".to_owned(), attributes)];
    if let Some(endpoint) = &base.endpoint {
        vars.push(("OTEL_EXPORTER_OTLP_ENDPOINT".to_owned(), endpoint.clone()));
        if !base.headers.is_empty() {
            let list: Vec<String> = base
                .headers
                .iter()
                .map(|(k, v)| format!("{k}={}", header_value(v)))
                .collect();
            vars.push(("OTEL_EXPORTER_OTLP_HEADERS".to_owned(), list.join(",")));
        }
    }
    vars
}

#[cfg(test)]
mod tests;
