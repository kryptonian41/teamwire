//! Every enum with a string form: `parse(as_str(x)) == x`, serde uses the
//! same string, and the forms are the ones the docs use.

use std::collections::HashSet;
use std::fmt::Debug;
use std::str::FromStr;

use serde::Serialize;
use serde::de::DeserializeOwned;
use teamwire_core::*;

fn round_trip<T>(all: &[T], as_str: fn(T) -> &'static str)
where
    T: Copy + PartialEq + Debug + Serialize + DeserializeOwned + FromStr,
    <T as FromStr>::Err: Debug,
{
    let mut seen = HashSet::new();
    for &value in all {
        let text = as_str(value);
        assert!(seen.insert(text), "{text} used twice");
        assert_eq!(text.parse::<T>().ok(), Some(value));
        let json = format!("\"{text}\"");
        assert_eq!(serde_json::to_string(&value).ok(), Some(json.clone()));
        assert_eq!(serde_json::from_str::<T>(&json).ok(), Some(value));
    }
    assert!("no-such-value".parse::<T>().is_err());
}

#[test]
fn every_enum_round_trips() {
    round_trip(MemberKind::ALL, MemberKind::as_str);
    round_trip(Mode::ALL, Mode::as_str);
    round_trip(MemberState::ALL, MemberState::as_str);
    round_trip(MessageType::ALL, MessageType::as_str);
    round_trip(DeliveryState::ALL, DeliveryState::as_str);
    round_trip(Route::ALL, Route::as_str);
    round_trip(DeliveryErrorCode::ALL, DeliveryErrorCode::as_str);
    round_trip(CompletionStatus::ALL, CompletionStatus::as_str);
    round_trip(ReportState::ALL, ReportState::as_str);
    round_trip(FinalSource::ALL, FinalSource::as_str);
    round_trip(EventType::ALL, EventType::as_str);
    round_trip(SubmitAck::ALL, SubmitAck::as_str);
    round_trip(PlaceState::ALL, PlaceState::as_str);
    round_trip(HostErrorCode::ALL, HostErrorCode::as_str);
    round_trip(ScreenState::ALL, ScreenState::as_str);
    round_trip(StartupOutcome::ALL, StartupOutcome::as_str);
    round_trip(SessionKind::ALL, SessionKind::as_str);
    round_trip(TrustPolicy::ALL, TrustPolicy::as_str);
    round_trip(IdentityChannel::ALL, IdentityChannel::as_str);
    round_trip(HarnessErrorCode::ALL, HarnessErrorCode::as_str);
    round_trip(ReleaseOutcome::ALL, ReleaseOutcome::as_str);
    round_trip(PlatformErrorCode::ALL, PlatformErrorCode::as_str);
    round_trip(TeamState::ALL, TeamState::as_str);
    round_trip(LeadRoute::ALL, LeadRoute::as_str);
}

#[test]
fn state_columns_use_the_documented_values() {
    let names = |all: Vec<&str>| all.join(" ");
    assert_eq!(
        names(MemberState::ALL.iter().map(|s| s.as_str()).collect()),
        "creating started failed removed"
    );
    assert_eq!(
        names(DeliveryState::ALL.iter().map(|s| s.as_str()).collect()),
        "pending handed_off unconfirmed undelivered acked"
    );
    assert_eq!(
        names(CompletionStatus::ALL.iter().map(|s| s.as_str()).collect()),
        "done blocked failed"
    );
    assert_eq!(
        names(ReportState::ALL.iter().map(|s| s.as_str()).collect()),
        "pending delivering delivered undelivered unconfirmed"
    );
    assert_eq!(Route::HostTyping.as_str(), "host_typing");
}

#[test]
fn event_types_are_the_names_in_core_md() {
    let documented = "team.created team.closing team.closed team.lead_adopted \
        member.creating member.started member.failed member.refused member.replaced member.removed member.reprompted \
        member.completed member.completion_refused member.held \
        message.sent message.refused \
        delivery.handed_off delivery.unconfirmed delivery.undelivered delivery.acked delivery.reminded delivery.resent \
        delivery.reentered \
        result.delivering result.delivered result.undelivered result.unconfirmed \
        watch.started watch.line watch.stopped \
        verify.recorded verify.waived \
        close.step close.refused \
        store.error";
    let ours: Vec<&str> = EventType::ALL.iter().map(|e| e.as_str()).collect();
    let theirs: Vec<&str> = documented.split_whitespace().collect();
    assert_eq!(ours, theirs);
}

#[test]
fn refusal_codes_match_the_mcp_error_codes() {
    let codes: Vec<&str> = [
        Refusal::Identity,
        Refusal::TypeNotAllowed,
        Refusal::RecipientNotAllowed,
        Refusal::UnknownRecipient,
        Refusal::UnknownTeam,
        Refusal::RecipientTeamClosed,
        Refusal::RemovedRecipient,
        Refusal::SelfMessage,
        Refusal::EmptyBody,
        Refusal::TooLong {
            limit: 800,
            got: 912,
        },
        Refusal::NoCommit,
        Refusal::CommitNotReachable,
        Refusal::NotAQuestion,
        Refusal::NotAskedOfYou,
        Refusal::TeamClosed,
        Refusal::RateLimited,
        Refusal::Repeat,
        Refusal::NotResendable,
    ]
    .iter()
    .map(Refusal::code)
    .collect();
    assert_eq!(
        codes.join(" "),
        "identity type_not_allowed recipient_not_allowed unknown_recipient unknown_team \
         recipient_team_closed removed_recipient self empty too_long no_commit commit_not_reachable not_a_question not_asked_of_you \
         team_closed rate_limited repeat not_resendable"
    );
}

#[test]
fn host_codes_map_to_delivery_codes() {
    for &code in HostErrorCode::ALL {
        let mapped = DeliveryErrorCode::from_host(code);
        let expected = match code {
            HostErrorCode::Unsupported | HostErrorCode::Other => "host_other".to_owned(),
            HostErrorCode::NotReady
            | HostErrorCode::Busy
            | HostErrorCode::Blocked
            | HostErrorCode::NotFound
            | HostErrorCode::Stalled
            | HostErrorCode::Timeout
            | HostErrorCode::BadArgument => format!("host_{}", code.as_str()),
        };
        assert_eq!(mapped.as_str(), expected);
    }
}

#[test]
fn spec_defaults_dispatch_and_trust_to_true() {
    let spec: TeamSpec = serde_json::from_str(
        r#"{"team":"tw","title":"t","children":[{"role":"api","title":"API","mode":"worktree"}]}"#,
    )
    .unwrap();
    let child = spec.children.first().unwrap();
    assert!(child.dispatch && child.accept_trust);
    assert!(child.kind.is_none() && child.owns.is_empty());
}
