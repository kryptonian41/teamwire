//! teamwire's shared vocabulary: names and ids, member, message, completion
//! and event types, the `Host`, `Harness`, `Platform` and `Clock` traits,
//! the adapter registry, and the sanitiser.
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
pub mod harness;
pub mod host;
pub mod member;
pub mod message;
pub mod names;
pub mod platform;
pub mod registry;
pub mod sanitize;
pub mod secret;
pub mod spec;
pub mod team;
pub mod timestamp;
pub mod who;

pub use completion::{Completion, CompletionStatus, FinalSource, ReportGaps, ReportState};
pub use config::{Config, OtelConfig};
pub use event::{Event, EventType};
pub use harness::{
    CallerSession, Harness, HarnessCaps, HarnessError, HarnessErrorCode, IdentityChannel,
    LaunchFile, LaunchSpec, Liveness, MemberLaunch, ScreenState, SessionKind, StartupOutcome,
    TrustPolicy, TurnEnd, TurnEndReply,
};
pub use host::{
    Host, HostCaps, HostError, HostErrorCode, Key, NoHost, Occupant, PlaceState, SpawnRequest,
    SubmitAck,
};
pub use member::{Member, MemberKind, MemberState, Mode};
pub use message::{
    Delivery, DeliveryError, DeliveryErrorCode, DeliveryState, Message, MessageType, Refusal,
    Route, SendRequest,
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
pub use spec::{MemberSpec, TeamSpec};
pub use strings::UnknownString;
pub use team::{LeadRoute, TeamState};
pub use timestamp::{Timestamp, TimestampError};
pub use who::{Actor, Recipient, Remote};
