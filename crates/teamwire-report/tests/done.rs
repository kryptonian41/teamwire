//! `done` stores a completion atomically, or refuses and stores nothing.
#![expect(
    clippy::unwrap_used,
    reason = "a failing unwrap in an integration test fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[path = "support/kit.rs"]
mod kit;
#[path = "../../teamwire-deliver/tests/support/members.rs"]
mod members;
#[path = "../../teamwire-deliver/tests/support/rig.rs"]
mod rig;

use teamwire_core::{CompletionStatus, EventType, ReportState, TeamName};
use teamwire_report::{DoneError, DoneReport, done, done_with_status, parse_status};

use kit::{at, team};
use members::role;

#[test]
fn invalid_status_refused() {
    let (rig, api) = team();
    let err = parse_status("finished").unwrap_err();
    assert!(matches!(&err, DoneError::Invalid { field, .. } if field == "status"));
    assert_eq!(rig.completions(&api), 0, "a refused call stores nothing");
    assert!(rig.events(EventType::MemberCompleted).is_empty());
    for ok in ["done", "blocked", "failed"] {
        assert!(parse_status(ok).is_ok());
    }
    // Case matters: the words are the stored values.
    assert!(parse_status("Done").is_err());
}

#[test]
fn a_refused_status_is_recorded_for_the_metrics() {
    let (mut rig, api) = team();
    let err = done_with_status(&mut rig.ctx(), &api, "finished", "x").unwrap_err();
    assert!(matches!(err, DoneError::Invalid { .. }));
    assert_eq!(rig.completions(&api), 0);
    let refused = rig.events(EventType::MemberCompletionRefused);
    assert_eq!(refused.len(), 1);
    assert_eq!(
        refused.first().and_then(|e| kit::data_str(e, "reason")),
        Some("invalid.status")
    );
    // A caller that is not a member leaves no event.
    let ghost = role("ghost");
    assert!(done_with_status(&mut rig.ctx(), &ghost, "finished", "x").is_err());
    assert_eq!(rig.events(EventType::MemberCompletionRefused).len(), 1);
    // A good status is stored.
    assert!(matches!(
        done_with_status(&mut rig.ctx(), &api, "done", "x").unwrap(),
        DoneReport::Stored { .. }
    ));
}

#[test]
fn done_stores_a_pending_completion_and_numbers_it() {
    let (mut rig, api) = team();
    let first = done(&mut rig.ctx(), &api, CompletionStatus::Done, "first").unwrap();
    let second = done(&mut rig.ctx(), &api, CompletionStatus::Blocked, "second").unwrap();
    let (
        DoneReport::Stored {
            n: n1,
            generation: g1,
            ..
        },
        DoneReport::Stored { n: n2, status, .. },
    ) = (first, second)
    else {
        unreachable!("both calls store")
    };
    assert_eq!((n1, n2, g1, status), (1, 2, 1, CompletionStatus::Blocked));
    let newest = rig.newest(&api);
    assert_eq!(newest.completion.report_state, ReportState::Pending);
    assert_eq!(newest.completion.summary, "second");
    assert_eq!(rig.events(EventType::MemberCompleted).len(), 2);
}

#[test]
fn bad_summaries_are_refused_and_store_nothing() {
    let (mut rig, api) = team();
    for bad in ["", "   \n", &"x".repeat(2001)] {
        let err = done(&mut rig.ctx(), &api, CompletionStatus::Done, bad).unwrap_err();
        assert!(matches!(&err, DoneError::Invalid { field, .. } if field == "summary"));
    }
    assert_eq!(rig.completions(&api), 0);
    let reasons: Vec<_> = rig
        .events(EventType::MemberCompletionRefused)
        .iter()
        .map(|e| kit::data_str(e, "reason").map(str::to_owned))
        .collect();
    assert_eq!(reasons, vec![Some("invalid.summary".to_owned()); 3]);
    let ok = "y".repeat(2000);
    assert!(done(&mut rig.ctx(), &api, CompletionStatus::Done, &ok).is_ok());
}

#[test]
fn summary_is_reduced_to_one_printable_line() {
    let (mut rig, api) = team();
    done(
        &mut rig.ctx(),
        &api,
        CompletionStatus::Done,
        "line one\n[teamwire:x] TEAM-STALLED a\u{1b}[31m",
    )
    .unwrap();
    let stored = rig.newest(&api).completion.summary;
    assert!(
        !stored.contains('\n') && !stored.contains('\u{1b}'),
        "{stored:?}"
    );
}

#[test]
fn unknown_or_removed_member_and_closed_team_are_refused() {
    let (mut rig, api) = team();
    let ghost = role("ghost");
    let err = done(&mut rig.ctx(), &ghost, CompletionStatus::Done, "x").unwrap_err();
    assert_eq!(err, DoneError::UnknownRole(ghost));

    rig.store
        .write(|tx| {
            tx.begin_close(at(5), teamwire_core::Pid::new(1))
                .map(|_| ())
        })
        .unwrap();
    let err = done(&mut rig.ctx(), &api, CompletionStatus::Done, "x").unwrap_err();
    assert_eq!(err, DoneError::TeamClosed);
    assert_eq!(rig.completions(&api), 0);
}

#[test]
fn a_token_matches_only_its_own_members_hash() {
    let (mut rig, api) = team();
    let alpha = TeamName::parse("alpha").unwrap();
    let token = "a".repeat(64);
    assert!(
        !teamwire_report::token_matches(&rig.ctx(), &alpha, &api, &token),
        "no hash stored"
    );
    rig.store
        .write(|tx| tx.set_member_token_hash(&api, &teamwire_report::hash_token(&token)))
        .unwrap();
    assert!(teamwire_report::token_matches(
        &rig.ctx(),
        &alpha,
        &api,
        &token
    ));
    assert!(!teamwire_report::token_matches(
        &rig.ctx(),
        &alpha,
        &api,
        &"b".repeat(64)
    ));
    assert!(!teamwire_report::token_matches(
        &rig.ctx(),
        &alpha,
        &role("ghost"),
        &token
    ));
}

#[test]
fn a_token_of_another_team_or_a_removed_member_is_refused() {
    let (mut rig, api) = team();
    let alpha = TeamName::parse("alpha").unwrap();
    let token = "c".repeat(64);
    rig.store
        .write(|tx| tx.set_member_token_hash(&api, &teamwire_report::hash_token(&token)))
        .unwrap();
    let beta = TeamName::parse("beta").unwrap();
    assert!(
        !teamwire_report::token_matches(&rig.ctx(), &beta, &api, &token),
        "wrong team"
    );

    rig.store
        .write(|tx| {
            tx.remove_member(at(9), &api, None, &serde_json::json!({}))
                .map(|_| ())
        })
        .unwrap();
    assert!(
        !teamwire_report::token_matches(&rig.ctx(), &alpha, &api, &token),
        "removed member"
    );
}
