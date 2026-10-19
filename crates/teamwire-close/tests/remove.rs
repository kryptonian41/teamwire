//! Removing one member (CLS-4, CLS-5, CLS-6): the same checks as a whole
//! close for that member, a passing verify record for its current HEAD, and
//! the closer's report.
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a person who edits files in a worktree"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::{Rig, git};
use teamwire_close::{CloseOpts, CloseReport, CloseTarget, close, plan_close_with};
use teamwire_core::{
    Clock, CompletionStatus, EventType, FinalSource, Host, Liveness, MemberKind, MemberState,
    PlaceState, Role,
};
use teamwire_store::{NewCompletion, NewVerifyRecord, Waiver};

fn asks(reasons: &[String], word: &str) -> bool {
    reasons.iter().any(|r| r.contains(word))
}

#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

fn remove(rig: &mut Rig, name: &str, opts: CloseOpts) -> CloseReport {
    close(&mut rig.ctx(), CloseTarget::Member(role(name)), opts)
}

fn head_of(rig: &Rig, name: &str) -> String {
    git(&rig.member(name).cwd, &["rev-parse", "HEAD"])
}

#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn verify_at(rig: &mut Rig, name: &str, head: &str, verdict: &str, flags: &[&str]) {
    let now = rig.clock.now();
    let record = NewVerifyRecord {
        role: role(name),
        head_sha: head.to_owned(),
        verdict: verdict.to_owned(),
        exit_code: Some(0),
        seconds: Some(1),
        flags: flags.iter().map(|f| (*f).to_owned()).collect(),
        log_sha256: None,
    };
    rig.store
        .write(|tx| tx.insert_verify_record(now, &record))
        .unwrap();
}

#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn commit_more(rig: &Rig, name: &str, file: &str) -> String {
    let cwd = rig.member(name).cwd;
    std::fs::write(cwd.join(file), "more\n").unwrap();
    git(&cwd, &["add", "-A"]);
    git(&cwd, &["commit", "-q", "-m", "more work"]);
    head_of(rig, name)
}

#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn report_done(rig: &mut Rig, name: &str) {
    let now = rig.clock.now();
    let generation = rig.member(name).generation;
    rig.store
        .write(|tx| {
            tx.insert_completion(
                now,
                &NewCompletion {
                    role: role(name),
                    generation,
                    status: CompletionStatus::Done,
                    summary: "reviewed".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
        })
        .unwrap();
}

/// An implementer with one commit, verified `PASS` at that commit.
fn verified_implementer() -> Rig {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    commit_more(&rig, "api", "lib.rs");
    let head = head_of(&rig, "api");
    verify_at(&mut rig, "api", &head, "PASS", &[]);
    rig
}

fn place_is_closed(rig: &Rig, name: &str) -> bool {
    rig.host
        .state(&rig.place_of(name), std::time::Duration::ZERO)
        .is_err()
}

#[test]
fn a_verified_implementer_is_removed_and_its_row_and_branch_stay() {
    let mut rig = verified_implementer();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let api = rig.member("api");
    let report = remove(&mut rig, "api", CloseOpts::default());

    assert!(report.refusals.is_empty(), "{report:?}");
    assert_eq!(report.stopped_at, None, "{report:?}");
    let names: Vec<&str> = report.steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(names, ["close_place", "remove_worktree", "mark_removed"]);
    assert!(!api.cwd.exists(), "the worktree went, without force");
    assert_eq!(
        git(&rig.repo, &["branch", "--list", "br-api"]).trim(),
        "br-api"
    );
    assert!(place_is_closed(&rig, "api"));
    // The rest of the team runs on.
    assert!(!place_is_closed(&rig, "scout"));
    assert_eq!(rig.member("scout").state, MemberState::Started);
    // CLS-6: the row is kept, marked, and says why.
    let row = rig.member("api");
    assert_eq!(row.state, MemberState::Removed);
    assert!(row.removed_at.is_some());
    assert_eq!(row.removed_note.as_deref(), Some("removed"));
    assert_eq!(report.branches_kept, ["br-api"]);
    let events = rig.events(EventType::MemberRemoved);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events.first().unwrap().data.get("unverified"),
        Some(&serde_json::json!(false))
    );

    // Asking again is not an error, and touches nothing.
    let again = remove(&mut rig, "api", CloseOpts::default());
    assert!(
        again.refusals.is_empty() && again.stopped_at.is_none(),
        "{again:?}"
    );
    assert!(
        again
            .steps
            .first()
            .unwrap()
            .detail
            .contains("already removed")
    );
    assert_eq!(rig.events(EventType::MemberRemoved).len(), 1);
}

#[test]
fn unverified_implementer_refused() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert!(
        asks(&report.refusals, "api has no verify record"),
        "{report:?}"
    );
    assert!(
        asks(&report.refusals, "teamwire verify alpha --role api"),
        "{report:?}"
    );
    assert!(asks(&report.refusals, "--unverified"), "{report:?}");
    assert!(report.steps.is_empty());
    assert_eq!(rig.member("api").state, MemberState::Started);
    assert!(rig.member("api").cwd.is_dir());
    assert!(!place_is_closed(&rig, "api"));

    // The override removes it, and says why it did.
    let opts = CloseOpts {
        unverified: true,
        ..CloseOpts::default()
    };
    let report = remove(&mut rig, "api", opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    let first = report.steps.first().unwrap();
    assert_eq!(first.step, "unverified");
    assert!(
        first
            .detail
            .starts_with("removing anyway (--unverified): api has no verify record"),
        "{first:?}"
    );
    assert_eq!(rig.member("api").state, MemberState::Removed);
    let removed = rig.events(EventType::MemberRemoved);
    assert_eq!(
        removed.first().unwrap().data.get("unverified"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn verify_record_for_old_head_refused() {
    let mut rig = verified_implementer();
    let old = head_of(&rig, "api");
    // A waiver for the old head is on record too. It does not carry over.
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.insert_waiver(
                now,
                &Waiver {
                    role: role("api"),
                    flag_id: "weakened-test".to_owned(),
                    head_sha: old.clone(),
                    reason: "fine".to_owned(),
                    created_at: now,
                },
            )
        })
        .unwrap();
    let new = commit_more(&rig, "api", "more.rs");
    assert_ne!(old, new);
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert!(asks(&report.refusals, "was verified at"), "{report:?}");
    assert!(
        asks(&report.refusals, "older HEAD does not carry over"),
        "{report:?}"
    );
    assert_eq!(rig.member("api").state, MemberState::Started);

    // Verified again at the new HEAD: removed.
    verify_at(&mut rig, "api", &new, "PASS", &[]);
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(rig.member("api").state, MemberState::Removed);
}

#[test]
fn flagged_implementer_removal_refused() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    let head = commit_more(&rig, "api", "lib.rs");
    // The check passed but a rule flagged a deleted test: `FLAGGED`, not `PASS`.
    verify_at(
        &mut rig,
        "api",
        &head,
        "FLAGGED",
        &["deleted-test:tests/a.rs"],
    );
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert!(asks(&report.refusals, "is FLAGGED, not PASS"), "{report:?}");
    assert!(asks(&report.refusals, "--waive"), "{report:?}");
    assert_eq!(rig.member("api").state, MemberState::Started);

    // A waiver alone changes no record: the lead verifies again, and that
    // run, which counts the waiver, records PASS.
    verify_at(&mut rig, "api", &head, "PASS", &["deleted-test:tests/a.rs"]);
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
}

#[test]
fn a_failed_check_and_a_missing_check_are_not_a_pass_either() {
    for verdict in ["FAIL", "NO-CHECK", "SKIPPED"] {
        let mut rig = Rig::new();
        rig.add_worktree_member("api", MemberKind::Implementer);
        let head = head_of(&rig, "api");
        verify_at(&mut rig, "api", &head, verdict, &[]);
        let report = remove(&mut rig, "api", CloseOpts::default());
        assert!(
            asks(&report.refusals, &format!("is {verdict}, not PASS")),
            "{verdict}: {report:?}"
        );
    }
}

#[test]
fn before_closer_report_refused() {
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.add_pane_member("wrap", MemberKind::Closer);
    // Any member, even one that needs no verify, waits for the closer.
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert!(
        asks(&report.refusals, "closer wrap has not reported yet"),
        "{report:?}"
    );
    assert_eq!(rig.member("scout").state, MemberState::Started);
    assert!(!place_is_closed(&rig, "scout"));

    // The closer itself is not held back by its own report.
    let opts = CloseOpts {
        unverified: true,
        ..CloseOpts::default()
    };
    let forced = remove(&mut rig, "scout", opts);
    assert_eq!(forced.stopped_at, None, "{forced:?}");
    assert!(
        forced
            .steps
            .first()
            .unwrap()
            .detail
            .contains("closer wrap has not reported yet")
    );

    // Once the closer reported, removal needs no override.
    rig.add_pane_member("scout2", MemberKind::Reviewer);
    report_done(&mut rig, "wrap");
    let report = remove(&mut rig, "scout2", CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(report.refusals.is_empty());
    let own = remove(&mut rig, "wrap", CloseOpts::default());
    assert_eq!(own.stopped_at, None, "{own:?}");
}

#[test]
fn a_closer_that_never_started_or_was_reprompted_is_judged_by_its_current_generation() {
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.add_pane_member("wrap", MemberKind::Closer);
    report_done(&mut rig, "wrap");
    let now = rig.clock.now();
    // The lead reprompted the closer with its findings: its earlier report
    // is not the report of the new generation.
    rig.store
        .write(|tx| tx.bump_generation(now, &role("wrap"), "look again"))
        .unwrap();
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert!(
        asks(&report.refusals, "closer wrap has not reported yet"),
        "{report:?}"
    );
}

#[test]
fn busy_member_needs_stop_and_stop_says_so() {
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.set_place_state("scout", PlaceState::Working);
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert!(asks(&report.refusals, "scout is busy"), "{report:?}");
    assert!(asks(&report.refusals, "--role scout --stop"), "{report:?}");
    assert_eq!(rig.member("scout").state, MemberState::Started);
    assert!(!place_is_closed(&rig, "scout"));

    let opts = CloseOpts {
        stop: true,
        ..CloseOpts::default()
    };
    let report = remove(&mut rig, "scout", opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    let stop = report.steps.iter().find(|s| s.step == "stop").unwrap();
    assert!(
        stop.detail.contains("that turn writes no report"),
        "{stop:?}"
    );
    assert!(place_is_closed(&rig, "scout"));
    assert_eq!(
        rig.member("scout").removed_note.as_deref(),
        Some("stopped while busy")
    );
    assert_eq!(
        rig.events(EventType::MemberRemoved)
            .first()
            .unwrap()
            .data
            .get("stopped"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn stop_never_overrides_uncommitted_work() {
    let mut rig = Rig::new();
    let api = rig.add_worktree_member("api", MemberKind::Implementer);
    let head = head_of(&rig, "api");
    verify_at(&mut rig, "api", &head, "PASS", &[]);
    std::fs::write(api.cwd.join("wip.txt"), "not committed\n").unwrap();
    let opts = CloseOpts {
        stop: true,
        unverified: true,
        ..CloseOpts::default()
    };
    let report = remove(&mut rig, "api", opts);
    assert!(
        asks(&report.refusals, "api has 1 uncommitted path"),
        "{report:?}"
    );
    assert!(api.cwd.join("wip.txt").is_file());
    assert_eq!(rig.member("api").state, MemberState::Started);
}

#[test]
fn an_unknown_state_refuses_without_stop_and_is_stopped_anyway_with_it() {
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.set_place_state("scout", PlaceState::Unknown);
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert!(
        asks(&report.refusals, "cannot tell whether scout is busy"),
        "{report:?}"
    );
    assert_eq!(rig.member("scout").state, MemberState::Started);

    let opts = CloseOpts {
        stop: true,
        ..CloseOpts::default()
    };
    let report = remove(&mut rig, "scout", opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    let stop = report.steps.iter().find(|s| s.step == "stop").unwrap();
    assert!(
        stop.detail.contains("state was unknown") && stop.detail.contains("stopped anyway"),
        "{stop:?}"
    );
    assert!(place_is_closed(&rig, "scout"));
    assert_eq!(rig.member("scout").state, MemberState::Removed);
}

#[test]
fn a_host_with_no_state_asks_the_harness_instead() {
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.set_place_state("scout", PlaceState::Unknown);

    rig.set_session_liveness("scout", Liveness::Alive(PlaceState::Working));
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert!(asks(&report.refusals, "scout is busy"), "{report:?}");

    rig.set_session_liveness("scout", Liveness::Alive(PlaceState::Blocked));
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert!(asks(&report.refusals, "dialog"), "{report:?}");

    // Both unknown stays unknown: a session the harness cannot read is no answer.
    rig.set_session_liveness("scout", Liveness::Unknown);
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert!(
        asks(&report.refusals, "cannot tell whether scout is busy"),
        "{report:?}"
    );

    rig.set_session_liveness("scout", Liveness::Alive(PlaceState::Idle));
    let report = remove(&mut rig, "scout", CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(report.refusals.is_empty(), "{report:?}");
    assert_eq!(rig.member("scout").state, MemberState::Removed);
    assert!(place_is_closed(&rig, "scout"));
}

#[test]
fn a_member_that_never_started_needs_no_verify_and_a_pane_member_no_worktree() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.set_member_state(
                now,
                &role("api"),
                MemberState::Failed,
                &serde_json::json!({}),
            )
        })
        .unwrap();
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(rig.member("api").state, MemberState::Removed);
}

#[test]
fn a_removal_that_git_refuses_stops_with_the_row_kept() {
    let mut rig = verified_implementer();
    let api = rig.member("api");
    // Git will not remove a worktree that holds a locked path; here, a
    // second worktree operation is impossible because the directory holds an
    // ignored file created after the checks. Simulate with a lock.
    git(
        &rig.repo,
        &["worktree", "lock", &api.cwd.display().to_string()],
    );
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert_eq!(
        report.stopped_at.as_deref(),
        Some("remove_worktree"),
        "{report:?}"
    );
    assert!(
        report
            .steps
            .last()
            .unwrap()
            .detail
            .contains("git worktree remove")
    );
    assert_eq!(
        rig.member("api").state,
        MemberState::Started,
        "the row is not marked removed"
    );
    assert!(api.cwd.is_dir());
    // Without a force flag anywhere.
    assert!(
        rig.platform
            .inner
            .runs()
            .iter()
            .all(|(c, _)| !c.args.iter().any(|a| a == "--force" || a == "-f"))
    );
}

#[test]
fn removing_needs_a_member_and_an_open_team() {
    let mut rig = Rig::without_repo();
    let report = remove(&mut rig, "nobody", CloseOpts::default());
    assert!(
        asks(&report.refusals, "no member nobody in team alpha"),
        "{report:?}"
    );
    // The plan says the same and changes nothing.
    let plan = plan_close_with(
        &rig.ctx(),
        CloseTarget::Member(role("nobody")),
        &CloseOpts::default(),
    );
    assert!(asks(&plan.refusals, "no member nobody"), "{plan:?}");
}

#[test]
fn the_dry_run_of_a_removal_lists_the_refusals_and_the_overrides_and_changes_nothing() {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    let plan = plan_close_with(
        &rig.ctx(),
        CloseTarget::Member(role("api")),
        &CloseOpts::default(),
    );
    assert!(asks(&plan.refusals, "api has no verify record"), "{plan:?}");
    let opts = CloseOpts {
        unverified: true,
        ..CloseOpts::default()
    };
    let plan = plan_close_with(&rig.ctx(), CloseTarget::Member(role("api")), &opts);
    assert!(plan.refusals.is_empty(), "{plan:?}");
    assert!(
        plan.steps.iter().any(|s| s.step == "unverified"),
        "{plan:?}"
    );
    let names: Vec<&str> = plan.steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(
        names,
        [
            "unverified",
            "close_place",
            "remove_worktree",
            "mark_removed",
            "branches_kept"
        ]
    );
    assert_eq!(rig.member("api").state, MemberState::Started);
    assert!(rig.member("api").cwd.is_dir());
}

#[test]
fn the_dry_run_of_a_pane_member_removal_names_no_worktree() {
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let plan = plan_close_with(
        &rig.ctx(),
        CloseTarget::Member(role("scout")),
        &CloseOpts::default(),
    );
    let names: Vec<&str> = plan.steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(names, ["close_place", "mark_removed"], "{plan:?}");
}

#[test]
fn a_member_that_turns_busy_before_its_place_closes_is_not_closed_without_stop() {
    // The first checks pass; then a turn starts. The re-check right before
    // the place closes sees it (CLS-3, CLS-4).
    for stop in [false, true] {
        let mut rig = Rig::without_repo();
        rig.add_pane_member("scout", MemberKind::Investigator);
        let host = rig.host.clone();
        let place = rig.place_of("scout");
        let opts = CloseOpts {
            stop,
            ..CloseOpts::default()
        };
        let report = teamwire_close::close_observed(
            &mut rig.ctx(),
            CloseTarget::Member(role("scout")),
            opts,
            &|step| {
                if step == "close_place" {
                    host.set_state(&place, PlaceState::Working);
                }
            },
        );
        if stop {
            assert_eq!(report.stopped_at, None, "{report:?}");
            assert!(place_is_closed(&rig, "scout"));
        } else {
            assert_eq!(
                report.stopped_at.as_deref(),
                Some("close_place"),
                "{report:?}"
            );
            assert!(
                report
                    .steps
                    .last()
                    .unwrap()
                    .detail
                    .contains("scout is busy")
            );
            assert!(!place_is_closed(&rig, "scout"), "the turn was not cut off");
            assert_eq!(rig.member("scout").state, MemberState::Started);
        }
    }
}

/// A close takes the lock from a second store handle, as another process would.
#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn take_the_close_lock(
    home: &std::path::Path,
    team: &teamwire_core::TeamName,
    at: teamwire_core::Timestamp,
) {
    let mut other = teamwire_store::Store::open_existing(home, team).unwrap();
    let taken = other
        .write(|tx| tx.begin_close(at, teamwire_core::Pid::new(1)))
        .unwrap();
    assert!(taken);
}

#[test]
fn a_removal_stops_when_a_close_takes_the_lock_after_the_plan() {
    // The lock is taken after the removal read the team and passed its gate:
    // the re-check before the place closes refuses (CLS-3).
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let report = {
        let (home, team, at) = (
            rig.home.path().to_path_buf(),
            rig.team().name,
            rig.clock.now(),
        );
        teamwire_close::close_observed(
            &mut rig.ctx(),
            CloseTarget::Member(role("scout")),
            CloseOpts::default(),
            &|step| {
                if step == "close_place" {
                    take_the_close_lock(&home, &team, at);
                }
            },
        )
    };
    assert_eq!(
        report.stopped_at.as_deref(),
        Some("close_place"),
        "{report:?}"
    );
    assert!(!place_is_closed(&rig, "scout"));
    assert_eq!(rig.member("scout").state, MemberState::Started);
}

#[test]
fn mark_removed_checks_the_lock_inside_its_own_write() {
    let mut rig = Rig::without_repo();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let report = {
        let (home, team, at) = (
            rig.home.path().to_path_buf(),
            rig.team().name,
            rig.clock.now(),
        );
        teamwire_close::close_observed(
            &mut rig.ctx(),
            CloseTarget::Member(role("scout")),
            CloseOpts::default(),
            &|step| {
                if step == "mark_removed" {
                    take_the_close_lock(&home, &team, at);
                }
            },
        )
    };
    assert_eq!(
        report.stopped_at.as_deref(),
        Some("mark_removed"),
        "{report:?}"
    );
    assert_eq!(
        rig.member("scout").state,
        MemberState::Started,
        "the row is kept"
    );
}

#[test]
fn a_removed_members_deleted_branch_is_not_reported_kept() {
    let mut rig = verified_implementer();
    let api = rig.member("api");
    git(&api.cwd, &["checkout", "-q", "--detach"]);
    git(&rig.repo, &["branch", "-D", "br-api"]);
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert!(report.refusals.is_empty(), "{report:?}");
    assert!(
        !report.branches_kept.iter().any(|b| b == "br-api"),
        "{report:?}"
    );
    // The deleted branch held the HEAD commit, so a rescue branch keeps it now.
    assert_eq!(report.branches_kept.len(), 1, "{report:?}");
}

#[test]
fn removing_a_member_on_a_detached_head_keeps_its_commits_on_a_rescue_branch() {
    let mut rig = verified_implementer();
    let api = rig.member("api");
    git(&api.cwd, &["checkout", "-q", "--detach"]);
    let sha = commit_more(&rig, "api", "detached.rs");
    verify_at(&mut rig, "api", &sha, "PASS", &[]);
    let rescue = format!(
        "teamwire/rescue/{}-api-{}",
        rig::TEAM,
        sha.get(..7).unwrap_or(&sha)
    );
    let report = remove(&mut rig, "api", CloseOpts::default());
    assert!(report.refusals.is_empty(), "{report:?}");
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(!api.cwd.exists());
    assert_eq!(git(&rig.repo, &["rev-parse", &rescue]), sha);
    assert_eq!(report.branches_kept, ["br-api".to_owned(), rescue]);
    assert_ne!(git(&rig.repo, &["for-each-ref", "--contains", &sha]), "");
}
