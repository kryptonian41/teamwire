//! herdr's error envelope and the table from `error.code` to `HostErrorCode`.
//! Only the code is read, never the message text (ADP-5).

use serde::Deserialize;
use teamwire_core::{HostError, HostErrorCode};

/// Longest herdr text copied into an error detail.
const DETAIL_MAX: usize = 300;

#[derive(Debug, Deserialize)]
pub(crate) struct ErrorBody {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

/// The `HostErrorCode` for one herdr `error.code`.
pub(crate) fn map_code(code: &str) -> HostErrorCode {
    match code {
        // `agent_not_idle`: a long read of an agent that is working.
        "agent_pane_busy" | "agent_not_idle" => HostErrorCode::Busy,
        "agent_not_ready" => HostErrorCode::NotReady,
        "agent_blocked" => HostErrorCode::Blocked,
        "agent_prompt_stalled" => HostErrorCode::Stalled,
        "timeout" => HostErrorCode::Timeout,
        "agent_not_found"
        | "pane_not_found"
        | "agent_pane_not_found"
        | "workspace_not_found"
        | "tab_not_found" => HostErrorCode::NotFound,
        "invalid_agent_argument"
        | "invalid_agent_name"
        | "invalid_agent_timeout"
        | "empty_agent_prompt"
        | "invalid_params"
        | "agent_name_taken" => HostErrorCode::BadArgument,
        _ => HostErrorCode::Other,
    }
}

/// Cuts `text` to at most `DETAIL_MAX` characters.
pub(crate) fn clip(text: &str) -> String {
    text.trim().chars().take(DETAIL_MAX).collect()
}

/// The error for a herdr error body. `agent_name_taken` keeps its code as
/// the start of the detail so launch can tell a name that is already live
/// from any other bad argument. Codes with no mapping keep the code too.
pub(crate) fn from_body(body: &ErrorBody) -> HostError {
    let code = map_code(&body.code);
    let detail = if code == HostErrorCode::Other || body.code == "agent_name_taken" {
        format!("{}: {}", body.code, clip(&body.message))
    } else {
        clip(&body.message)
    };
    HostError::new(code, detail)
}
