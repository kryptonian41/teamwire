//! Event rows: one per state change, the source of every metric.

use serde::{Deserialize, Serialize};

use crate::names::{Role, Ulid};
use crate::strings::string_enum;
use crate::timestamp::Timestamp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub ulid: Ulid,
    pub time: Timestamp,
    pub kind: EventType,
    pub role: Option<Role>,
    pub subject: Option<Ulid>,
    pub data: serde_json::Value,
    pub writer_version: String,
}

string_enum! {
    /// A closed list. The strings are the `type` column and the names in
    /// `docs/metrics.md` section 1.
    pub enum EventType {
        TeamCreated => "team.created",
        TeamClosing => "team.closing",
        TeamClosed => "team.closed",
        TeamLeadAdopted => "team.lead_adopted",
        MemberCreating => "member.creating",
        MemberStarted => "member.started",
        MemberFailed => "member.failed",
        MemberRefused => "member.refused",
        MemberReplaced => "member.replaced",
        MemberRemoved => "member.removed",
        MemberReprompted => "member.reprompted",
        MemberHandoff => "member.handoff",
        MemberCompleted => "member.completed",
        MemberCompletionRefused => "member.completion_refused",
        MemberHeld => "member.held",
        MessageSent => "message.sent",
        MessageRefused => "message.refused",
        DeliveryHandedOff => "delivery.handed_off",
        DeliveryUnconfirmed => "delivery.unconfirmed",
        DeliveryUndelivered => "delivery.undelivered",
        DeliveryAcked => "delivery.acked",
        DeliveryReminded => "delivery.reminded",
        DeliveryResent => "delivery.resent",
        DeliveryReentered => "delivery.reentered",
        ResultDelivering => "result.delivering",
        ResultDelivered => "result.delivered",
        ResultUndelivered => "result.undelivered",
        ResultUnconfirmed => "result.unconfirmed",
        WatchStarted => "watch.started",
        WatchLine => "watch.line",
        WatchStopped => "watch.stopped",
        VerifyRecorded => "verify.recorded",
        VerifyWaived => "verify.waived",
        CloseStep => "close.step",
        CloseRefused => "close.refused",
        StoreError => "store.error",
    }
}
