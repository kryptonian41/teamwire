//! The start of one member: the `pi` argv, the token file, and the
//! extension that runs teamwire's hooks inside pi.
//!
//! Nothing is written under pi's own directory: the extension is loaded with
//! `-e` from the member's directory. The token goes only into
//! `<member_dir>/token` (mode 0600), and argv names no secret, because argv
//! is readable by every process on the machine (ADR 0011).

use std::path::Path;

use teamwire_core::{
    Config, HarnessError, HarnessErrorCode, IdentityChannel, LaunchFile, LaunchSpec, MemberLaunch,
};

use crate::extension::{self, Values};

const PRIVATE: u32 = 0o600;

/// The extension's file name inside the member's directory.
pub(crate) const EXTENSION_FILE: &str = "pi-extension.ts";

/// The argv and files of a start. A resume is the same command: pi's
/// `--session-id` opens the session when it exists and creates it otherwise.
pub(crate) fn build(
    pi_bin: &str,
    sessions_dir: &Path,
    m: &MemberLaunch,
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
    // pi appends a missing file's path as if it were the prompt text, so a
    // missing contract is refused here rather than sent as its own name.
    if !m.contract_path.is_file() {
        return Err(HarnessError::new(
            HarnessErrorCode::Io,
            format!("the contract {} does not exist", m.contract_path.display()),
        ));
    }
    let token_path = m.member_dir.join("token");
    let extension_path = m.member_dir.join(EXTENSION_FILE);
    let home = m.home_from_member_dir();
    let hook_timeout_ms = Config::default().hook_timeout_s.saturating_mul(1000);
    let script = extension::render(&Values {
        teamwire_bin: &text(&m.teamwire_bin),
        home: home.as_deref(),
        team: m.team.as_str(),
        role: m.role.as_str(),
        token_file: &text(&token_path),
        state_dir: &text(sessions_dir),
        hook_timeout_ms,
    });

    let mut argv = vec![
        pi_bin.to_owned(),
        "--session-id".to_owned(),
        m.session_id.as_str().to_owned(),
        "--session-dir".to_owned(),
        text(sessions_dir),
        // A folder's own `.pi` extensions, MCP servers and settings run as
        // the member with no review, the case INV-25 refuses for Claude.
        // Skipping them also keeps pi's project-trust dialog from opening.
        "--no-approve".to_owned(),
        "--extension".to_owned(),
        text(&extension_path),
        "--append-system-prompt".to_owned(),
        text(&m.contract_path),
    ];
    if let Some(model) = &m.model {
        argv.extend(["--model".to_owned(), model.clone()]);
    }
    if let Some(effort) = &m.effort {
        argv.extend(["--thinking".to_owned(), effort.clone()]);
    }

    Ok(LaunchSpec {
        argv,
        // The host cannot be relied on to pass environment (ADR 0008), so
        // identity travels in the extension and nothing is set here.
        env: Vec::new(),
        files: vec![
            LaunchFile {
                path: token_path,
                bytes: token.into_bytes(),
                mode: PRIVATE,
            },
            LaunchFile {
                path: extension_path,
                bytes: script.into_bytes(),
                mode: PRIVATE,
            },
        ],
        identity_channel: IdentityChannel::HarnessConfig,
    })
}

fn text(p: &Path) -> String {
    p.display().to_string()
}
