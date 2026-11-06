//! Messages, their deliveries, and why a send was refused.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::host::{HostError, HostErrorCode};
use crate::names::{DisplayId, TeamName, Ulid};
use crate::strings::string_enum;
use crate::timestamp::Timestamp;
use crate::who::{Actor, Recipient};

string_enum! {
    pub enum MessageType {
        Notice => "notice",
        Finding => "finding",
        Question => "question",
        Answer => "answer",
        Reprompt => "reprompt",
        Note => "note",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendRequest {
    pub team: TeamName,
    /// Proven identity, not a claim.
    pub actor: Actor,
    pub to: Vec<Recipient>,
    pub kind: MessageType,
    pub body: String,
    /// Required for `Notice`.
    pub commit: Option<String>,
    /// Required for `Answer`.
    pub re: Option<DisplayId>,
    /// Idempotency key, unique per sender.
    pub client_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub ulid: Ulid,
    pub display_id: DisplayId,
    pub sender: Actor,
    pub seq: u64,
    pub kind: MessageType,
    pub re: Option<Ulid>,
    pub commit: Option<String>,
    pub body: String,
    pub body_sha256: [u8; 32],
    pub created_at: Timestamp,
}

string_enum! {
    pub enum DeliveryState {
        Pending => "pending",
        HandedOff => "handed_off",
        Unconfirmed => "unconfirmed",
        Undelivered => "undelivered",
        Acked => "acked",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub message: Ulid,
    pub recipient: Recipient,
    pub state: DeliveryState,
    pub attempts: u32,
    pub route: Option<Route>,
    pub error: Option<DeliveryError>,
    pub acked_at: Option<Timestamp>,
}

string_enum! {
    /// The mechanism that carried an item, not a product name.
    /// `HarnessPush` is a harness that puts text into its own session
    /// (opencode `prompt_async`, Claude Channels); no v1 adapter uses it.
    pub enum Route {
        HostTyping => "host_typing",
        HarnessPush => "harness_push",
        Inbox => "inbox",
        Watch => "watch",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{code}: {detail}")]
pub struct DeliveryError {
    pub code: DeliveryErrorCode,
    pub detail: String,
}

impl DeliveryError {
    pub fn new(code: DeliveryErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl From<HostError> for DeliveryError {
    fn from(e: HostError) -> Self {
        Self {
            code: DeliveryErrorCode::from_host(e.code),
            detail: e.detail,
        }
    }
}

string_enum! {
    pub enum DeliveryErrorCode {
        // from the locator (ADR 0006)
        NoLiveProcess => "no_live_process",
        NoPlace => "no_place",
        HostOtherSession => "host_other_session",
        HostNoSession => "host_no_session",
        HostListingFailed => "host_listing_failed",
        TwoPlaces => "two_places",
        SingleSource => "single_source",
        BackgroundSession => "background_session",
        SessionBlocked => "session_blocked",
        NotAtInputPrompt => "not_at_input_prompt",
        // from the registry: a recorded adapter id this binary does not have
        UnknownAdapter => "unknown_adapter",
        // from the host
        HostNotReady => "host_not_ready",
        HostBusy => "host_busy",
        HostBlocked => "host_blocked",
        HostNotFound => "host_not_found",
        HostStalled => "host_stalled",
        HostTimeout => "host_timeout",
        HostBadArgument => "host_bad_argument",
        HostOther => "host_other",
        // from teamwire
        RecipientRemoved => "recipient_removed",
        TeamClosed => "team_closed",
        NoHost => "no_host",
        /// A result left in `delivering` past `delivering_stale_s`: the hook
        /// that was delivering it was killed (REP-14).
        DeliveryInterrupted => "delivery_interrupted",
        /// The turn-end hook's delivery budget ran out before this call, so
        /// nothing was typed (ADR 0007 point 6a).
        BudgetExhausted => "budget_exhausted",
    }
}

impl DeliveryErrorCode {
    /// The delivery code for a host's error code. A host that does not
    /// support a call is reported as `host_other`.
    pub fn from_host(code: HostErrorCode) -> Self {
        match code {
            HostErrorCode::NotReady => Self::HostNotReady,
            HostErrorCode::Busy => Self::HostBusy,
            HostErrorCode::Blocked => Self::HostBlocked,
            HostErrorCode::NotFound => Self::HostNotFound,
            HostErrorCode::Stalled => Self::HostStalled,
            HostErrorCode::Timeout => Self::HostTimeout,
            HostErrorCode::BadArgument => Self::HostBadArgument,
            HostErrorCode::Unsupported | HostErrorCode::Other => Self::HostOther,
        }
    }
}

/// Why a send was refused before anything was stored (MSG-1..MSG-3, MSG-9, MSG-14, MSG-15).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Refusal {
    #[error("the caller's identity does not match")]
    Identity,
    #[error("this kind of member may not send this type")]
    TypeNotAllowed,
    #[error("this recipient may not receive this type from you")]
    RecipientNotAllowed,
    /// The user named the lead together with a member (ADR 0040). The code
    /// is `recipient_not_allowed`; only the text differs.
    #[error("the user's note to the lead goes as its own message: send it with --to lead alone")]
    UserLeadWithMember,
    #[error("no such member")]
    UnknownRecipient,
    #[error("no such team")]
    UnknownTeam,
    #[error("the recipient's team is closing or closed")]
    RecipientTeamClosed,
    #[error("the member was removed")]
    RemovedRecipient,
    #[error("you cannot send to yourself")]
    SelfMessage,
    #[error("the text is empty")]
    EmptyBody,
    #[error("the text is {got} characters; the limit is {limit}")]
    TooLong { limit: usize, got: usize },
    #[error("a notice needs a commit")]
    NoCommit,
    #[error("the commit is not an ancestor of your HEAD")]
    CommitNotReachable,
    #[error("re does not name a question")]
    NotAQuestion,
    #[error("re names a question asked of someone else")]
    NotAskedOfYou,
    #[error("the team is closing or closed")]
    TeamClosed,
    #[error("more than 5 messages to one recipient in a minute")]
    RateLimited,
    #[error("the same text went to the same recipient within 10 minutes and was delivered")]
    Repeat,
    #[error(
        "only the sender or the lead may resend, and only to a recipient in a resendable state"
    )]
    NotResendable,
}

impl Refusal {
    /// The code after `refused.` in the MCP error (`docs/api/mcp.md`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::TypeNotAllowed => "type_not_allowed",
            Self::RecipientNotAllowed | Self::UserLeadWithMember => "recipient_not_allowed",
            Self::UnknownRecipient => "unknown_recipient",
            Self::UnknownTeam => "unknown_team",
            Self::RecipientTeamClosed => "recipient_team_closed",
            Self::RemovedRecipient => "removed_recipient",
            Self::SelfMessage => "self",
            Self::EmptyBody => "empty",
            Self::TooLong { .. } => "too_long",
            Self::NoCommit => "no_commit",
            Self::CommitNotReachable => "commit_not_reachable",
            Self::NotAQuestion => "not_a_question",
            Self::NotAskedOfYou => "not_asked_of_you",
            Self::TeamClosed => "team_closed",
            Self::RateLimited => "rate_limited",
            Self::Repeat => "repeat",
            Self::NotResendable => "not_resendable",
        }
    }
}
