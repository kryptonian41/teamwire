//! The error shape of `docs/api/mcp.md`: a code, a message and whether a
//! retry can help.

use serde_json::{Value, json};
use teamwire_msg::SendError;
use teamwire_report::DoneError;
use teamwire_store::StoreError;

/// A refused or failed call, as the caller sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolFailure {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) retry: bool,
}

impl ToolFailure {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            retry: false,
        }
    }

    pub(crate) fn identity(message: impl Into<String>) -> Self {
        Self::new("refused.identity", message)
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid.argument", message)
    }

    /// `{"error": {"code", "message", "retry"}}`.
    pub(crate) fn to_json(&self) -> Value {
        json!({ "error": { "code": self.code, "message": self.message, "retry": self.retry } })
    }
}

impl From<StoreError> for ToolFailure {
    fn from(e: StoreError) -> Self {
        let (code, retry) = match &e {
            StoreError::Busy => ("store.busy", true),
            StoreError::Full { .. } => ("store.full", false),
            StoreError::SchemaNewer { .. } => ("store.schema_newer", false),
            StoreError::SchemaOlder { .. }
            | StoreError::NotTeamwire
            | StoreError::Corrupt(_)
            | StoreError::Constraint(_)
            | StoreError::NoDatabase { .. }
            | StoreError::Io(_) => ("internal", false),
        };
        Self {
            code: code.to_owned(),
            message: e.to_string(),
            retry,
        }
    }
}

impl From<SendError> for ToolFailure {
    fn from(e: SendError) -> Self {
        match e {
            SendError::Refused(refusal) => Self {
                code: format!("refused.{}", refusal.code()),
                message: refusal.to_string(),
                retry: refusal.code() == "rate_limited",
            },
            SendError::NotResendable(why) => Self::new("refused.not_resendable", why),
            SendError::InvalidArgument(why) => Self::invalid(why),
            SendError::Store(e) => e.into(),
            SendError::Internal(why) => Self::new("internal", why),
        }
    }
}

/// `docs/api/mcp.md` has no code for an unknown role or for a bad summary.
/// A role the store does not know cannot have been proven, so it is
/// `refused.identity`; every malformed field other than the status is
/// `invalid.argument`.
impl From<DoneError> for ToolFailure {
    fn from(e: DoneError) -> Self {
        match e {
            DoneError::Invalid { field, message } if field == "status" => {
                Self::new("invalid.status", message)
            }
            DoneError::Invalid { field, message } => Self::invalid(format!("{field}: {message}")),
            DoneError::TeamClosed => Self::new("refused.team_closed", e.to_string()),
            DoneError::UnknownRole(role) => Self::identity(format!("no member {role}")),
            DoneError::Store(e) => e.into(),
            DoneError::Internal(why) => Self::new("internal", why),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use teamwire_core::Refusal;

    #[test]
    fn a_rate_limit_is_the_only_refusal_worth_retrying() {
        let limited = ToolFailure::from(SendError::Refused(Refusal::RateLimited));
        assert_eq!(limited.code, "refused.rate_limited");
        assert!(limited.retry);
        assert!(!ToolFailure::from(SendError::Refused(Refusal::Repeat)).retry);
    }

    #[test]
    fn store_errors_carry_their_documented_codes() {
        assert_eq!(ToolFailure::from(StoreError::Busy).code, "store.busy");
        assert!(ToolFailure::from(StoreError::Busy).retry);
        let full = ToolFailure::from(StoreError::Full {
            path: "/v/team.db".into(),
        });
        assert_eq!(full.code, "store.full");
        assert!(full.message.contains("/v/team.db"), "{}", full.message);
        let newer = StoreError::SchemaNewer { db: 9, binary: 1 };
        assert_eq!(ToolFailure::from(newer).code, "store.schema_newer");
    }

    #[test]
    fn done_errors_without_a_documented_code_map_as_the_report_says() {
        let bad_status = DoneError::Invalid {
            field: "status".to_owned(),
            message: "no".to_owned(),
        };
        assert_eq!(ToolFailure::from(bad_status).code, "invalid.status");
        let bad_summary = DoneError::Invalid {
            field: "summary".to_owned(),
            message: "too long".to_owned(),
        };
        assert_eq!(ToolFailure::from(bad_summary).code, "invalid.argument");
    }
}
