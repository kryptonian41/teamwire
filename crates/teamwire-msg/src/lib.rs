//! The six message types and their rules (MSG-1..MSG-3, MSG-9, MSG-14,
//! MSG-15): proving the sender, send, resend, inbox, ack and log.
//!
//! Never: deliver before the message and its delivery rows are committed;
//! mark a delivery `acked` except from the recipient's own inbox or ack;
//! store anything for a refused send.
//!
//! Invariants: a sender's key never makes a second message; `seq` and
//! display ids come from counters inside the insert transaction; delivery
//! states only move forward and `acked` is final.

mod attempt;
mod inbox;
mod lead;
mod log;
mod prove;
mod resend;
mod roster;
mod rules;
mod send;

use std::fmt;

use serde::{Deserialize, Serialize};
use teamwire_core::{
    Actor, DeliveryErrorCode, DeliveryState, DisplayId, MessageType, Recipient, Refusal, Role,
    Timestamp, Ulid,
};
use teamwire_store::StoreError;
use thiserror::Error;

pub use inbox::{ack, inbox};
pub use lead::{AdoptReport, adopt_lead};
pub use log::log;
pub use prove::{prove_actor, token_hash};
pub use resend::resend;
pub use roster::{RosterEntry, roster};
pub use send::{
    Stored, parse_type_logged, prove_actor_logged, record_refusal, send, send_with, watch_running,
};

/// What the calling process says about itself, read from its environment
/// and arguments. `prove_actor` turns it into a proven `Actor` (ADR 0008).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CallerEnv {
    /// `TEAMWIRE_TEAM`.
    pub team: Option<String>,
    /// `TEAMWIRE_ROLE`.
    pub role: Option<String>,
    /// `TEAMWIRE_TOKEN`. Never printed.
    pub token: Option<String>,
    /// The claimed sender: `--from`, a role or `lead`.
    pub from: Option<String>,
}

impl fmt::Debug for CallerEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallerEnv")
            .field("team", &self.team)
            .field("role", &self.role)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("from", &self.from)
            .finish()
    }
}

/// One recipient's state in a send, resend or log row. `Skipped` appears
/// only in a resend, for a recipient that was already reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipientState {
    Pending,
    HandedOff,
    Unconfirmed,
    Undelivered,
    Acked,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientReport {
    pub to: Recipient,
    pub state: RecipientState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<DeliveryErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
}

/// The output of `team_send` and `team_resend` (`docs/api/mcp.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendReport {
    pub id: Ulid,
    pub display_id: DisplayId,
    pub seq: u64,
    pub deliveries: Vec<RecipientReport>,
    pub duplicate_of_key: bool,
}

/// Why a send, resend, inbox, ack or log call failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SendError {
    #[error("refused.{code}: {refusal}", code = .0.code(), refusal = .0)]
    Refused(Refusal),
    /// `team_resend` on a message the caller did not send (and is not the
    /// lead), or with no recipient in a resendable state.
    #[error("refused.not_resendable: {0}")]
    NotResendable(String),
    #[error("invalid.argument: {0}")]
    InvalidArgument(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("internal: {0}")]
    Internal(String),
}

/// One message in `team_inbox`'s output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    pub display_id: DisplayId,
    pub id: Ulid,
    pub from: Actor,
    #[serde(rename = "type")]
    pub kind: MessageType,
    pub time: Timestamp,
    pub re: Option<DisplayId>,
    pub commit: Option<String>,
    pub text: String,
    pub hash_ok: bool,
}

/// What `ack` did with one id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AckOutcome {
    Acked,
    Already,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AckResult {
    pub id: DisplayId,
    pub outcome: AckOutcome,
}

/// Which messages `log` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFilter {
    /// A member sees only messages it sent or received; the lead sees all.
    pub viewer: Actor,
    /// Only messages to or from this role.
    pub role: Option<Role>,
    pub limit: Option<usize>,
}

/// One row of `teamwire log` and `team_log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRow {
    pub display_id: DisplayId,
    pub time: Timestamp,
    #[serde(rename = "type")]
    pub kind: MessageType,
    pub from: Actor,
    pub to: Vec<Recipient>,
    /// At most 300 characters, sanitised.
    pub excerpt: String,
    pub deliveries: Vec<RecipientReport>,
}

impl From<DeliveryState> for RecipientState {
    fn from(state: DeliveryState) -> Self {
        match state {
            DeliveryState::Pending => Self::Pending,
            DeliveryState::HandedOff => Self::HandedOff,
            DeliveryState::Unconfirmed => Self::Unconfirmed,
            DeliveryState::Undelivered => Self::Undelivered,
            DeliveryState::Acked => Self::Acked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_env_debug_hides_the_token() {
        let env = CallerEnv {
            token: Some("tok-secret-1".to_owned()),
            ..CallerEnv::default()
        };
        assert!(!format!("{env:?}").contains("tok-secret-1"));
    }

    #[test]
    fn refusal_prints_its_mcp_code() {
        let e = SendError::Refused(Refusal::TooLong {
            limit: 800,
            got: 912,
        });
        assert_eq!(
            e.to_string(),
            "refused.too_long: the text is 912 characters; the limit is 800"
        );
    }
}
