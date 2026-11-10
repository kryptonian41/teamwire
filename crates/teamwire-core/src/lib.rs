//! teamwire's shared vocabulary: names and ids, member, message, completion
//! and event types, the `Host`, `Harness`, `LeadSetup`, `Platform` and
//! `Clock` traits, the adapter registry, the sanitiser, screen text without
//! its style codes, and the `info/exclude` line edits.
//!
//! Never: do I/O, read the clock, spawn a process, or name a product. An
//! adapter is known only by its string id and its capability record.
//!
//! Invariants: a name or id in hand always obeys its rule (it is checked in
//! `parse`); every enum stored or printed has one fixed string per value,
//! and `parse(as_str(x)) == x`; `Host::submit` takes only a `OneLine`.

mod strings;

pub mod completion;
pub mod config;
pub mod event;
pub mod exclude;
pub mod harness;
pub mod host;
pub mod lead_setup;
pub mod member;
pub mod message;
pub mod models;
pub mod names;
pub mod platform;
pub mod registry;
pub mod sanitize;
pub mod screen;
pub mod secret;
pub mod spec;
pub mod team;
pub mod timestamp;
pub mod tool_call;
pub mod usage;
pub mod who;

pub use completion::{Completion, CompletionStatus, FinalSource, ReportGaps, ReportState};
pub use config::{Config, OtelConfig};
pub use event::{Event, EventType};
pub use harness::{
    CallerSession, DEFAULT_HANDOFF_PCT, Fill, Harness, HarnessCaps, HarnessError, HarnessErrorCode,
    IdentityChannel, LaunchFile, LaunchSpec, Liveness, MIN_HANDOFF_TOKENS, MemberLaunch,
    ScreenState, SessionKind, StartupOutcome, TrustPolicy, TurnEnd, TurnEndReply, team_scratch_dir,
};
pub use host::{
    Host, HostCaps, HostError, HostErrorCode, Key, NoHost, Occupant, PlaceState, SpawnRequest,
    SubmitAck,
};
pub use lead_setup::{
    ApplyError, CheckOutcome, DoctorCheck, LeadChange, LeadCommand, LeadPlan, LeadSetup,
    LeadSetupError, LeadStatus, LeadStep, LeadTarget,
};
pub use member::{Member, MemberKind, MemberState, Mode};
pub use message::{
    Delivery, DeliveryError, DeliveryErrorCode, DeliveryState, Message, MessageType, Refusal,
    Route, SendRequest,
};
pub use models::{
    CatalogSource, Evidence, EvidenceEntry, EvidenceKind, ModelCatalog, ModelInfo, TaskClass,
};
pub use names::{
    Address, DisplayId, HarnessId, HostId, NameError, Pid, Place, Role, SessionId, TeamName, Ulid,
};
pub use platform::{
    Clock, Command, HoldInfo, Platform, PlatformError, PlatformErrorCode, ReleaseOutcome,
    RunOutput, SleepHold,
};
pub use registry::{Registry, RegistryError};
pub use sanitize::{OneLine, one_line};
pub use secret::{RedactedEnv, is_secret_name};
pub use spec::{MemberSpec, REFS_MAX, TeamSpec, TicketRef};
pub use strings::UnknownString;
pub use team::{LeadRoute, TeamState};
pub use timestamp::{Timestamp, TimestampError};
pub use tool_call::{ToolAction, ToolCall};
pub use usage::{SessionUsage, TokenCounts};
pub use who::{Actor, Recipient, Remote};
