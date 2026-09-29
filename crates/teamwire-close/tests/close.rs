//! `plan_close` and `close`: the refusals that come before any step, then
//! the fixed order, the close lock and the re-checks under it.
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a person who edits files in a worktree"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::Rig;
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close};
use teamwire_core::{EventType, Host, HostError, HostErrorCode, MemberKind, PlaceState, TeamState};
use teamwire_fakes::Fault;

fn asks(reasons: &[String], word: &str) -> bool {
    reasons.iter().any(|r| r.contains(word))
}

#[test]
fn busy_member_refuses() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.add_worktree_member("api", MemberKind::Implementer);
    rig.set_place_state("api", PlaceState::Working);

    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(asks(&plan.refusals, "api is busy"), "{:?}", plan.refusals);
    assert!(
        !asks(&plan.refusals, "scout"),
        "an idle member is not named: {:?}",
        plan.refusals
    );

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(asks(&report.refusals, "api is busy"), "{report:?}");
    assert!(
        report.steps.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    // Nothing changed: the team is open, both places and the worktree are there.
    assert_eq!(rig.team().state, TeamState::Open);
    assert_eq!(
        rig.host
            .state(&rig.place_of("scout"), std::time::Duration::ZERO),
        Ok(PlaceState::Idle)
    );
    assert!(rig.member("api").cwd.is_dir());
    assert!(!rig.archive_root().exists());
    assert_eq!(rig.events(EventType::CloseRefused).len(), 1);
    assert!(rig.events(EventType::TeamClosing).is_empty());
}

#[test]
fn a_dialog_waiting_for_an_answer_refuses_too() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.set_place_state("scout", PlaceState::Blocked);
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(asks(&plan.refusals, "dialog"), "{:?}", plan.refusals);
}

#[test]
fn unknown_busy_state_refuses() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.add_pane_member("rev", MemberKind::Reviewer);
    // One host that reports no state, one that fails outright.
    rig.set_place_state("scout", PlaceState::Unknown);
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(
        asks(&plan.refusals, "cannot tell whether scout is busy"),
        "{:?}",
        plan.refusals
    );

    rig.set_place_state("scout", PlaceState::Idle);
    rig.host.inject_always(
        "state",
        Fault::Fail(HostError::new(
            HostErrorCode::Other,
            "the herdr socket is gone",
        )),
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        asks(&report.refusals, "cannot tell whether scout is busy"),
        "{report:?}"
    );
    assert!(
        asks(&report.refusals, "cannot tell whether rev is busy"),
        "{report:?}"
    );
    assert!(asks(&report.refusals, "herdr socket is gone"), "{report:?}");
    assert_eq!(rig.team().state, TeamState::Open);
}

#[test]
fn a_place_the_host_no_longer_knows_is_not_busy() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.host.inject_always(
        "state",
        Fault::Fail(HostError::new(HostErrorCode::NotFound, "no such place")),
    );
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
}

#[test]
fn dirty_worktree_refuses() {
    let mut rig = Rig::new();
    let api = rig.add_worktree_member("api", MemberKind::Implementer);
    // The brief's copy is not work: TASK.md never counts, tracked or not.
    std::fs::write(api.cwd.join("TASK.md"), "brief\n").unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);

    std::fs::write(api.cwd.join("notes.txt"), "not committed\n").unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(
        asks(&plan.refusals, "api has 1 uncommitted path"),
        "{:?}",
        plan.refusals
    );

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(asks(&report.refusals, "uncommitted"), "{report:?}");
    assert!(
        api.cwd.join("notes.txt").is_file(),
        "the uncommitted file is untouched"
    );
    assert_eq!(rig.team().state, TeamState::Open);
}

#[test]
fn git_that_cannot_say_refuses_rather_than_guess() {
    let mut rig = Rig::new();
    let api = rig.add_worktree_member("api", MemberKind::Implementer);
    // Break the checkout's link to its repository: the directory is there,
    // but `git status` fails in it.
    std::fs::write(api.cwd.join(".git"), "gitdir: /nowhere\n").unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(
        asks(
            &plan.refusals,
            "cannot tell whether api has uncommitted work"
        ),
        "{:?}",
        plan.refusals
    );
}

#[test]
fn a_removed_member_is_not_asked_about() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.set_place_state("scout", PlaceState::Working);
    let role = teamwire_core::Role::parse("scout").unwrap();
    let now = teamwire_core::Clock::now(&rig.clock);
    rig.store
        .write(|tx| tx.remove_member(now, &role, Some("removed"), &serde_json::json!({})))
        .unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Team);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
}

// ---- the sequence, the lock and the archive (U2) ----

use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use rig::{entries, git};
use sha2::{Digest, Sha256};
use teamwire_close::close_observed;
use teamwire_core::{Clock, HoldInfo, Pid, ReleaseOutcome, Role};
use teamwire_fakes::HoldEvent;
use teamwire_store::{Ctx, Store};

const ORDER: [&str; 9] = [
    "lock",
    "archive",
    "verify_archive",
    "remove_worktrees",
    "close_places",
    "release_hold",
    "finish",
    "archive_final",
    "delete_state",
];

fn step_names(report: &teamwire_close::CloseReport) -> Vec<&str> {
    report.steps.iter().map(|s| s.step.as_str()).collect()
}

#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
/// A team with an implementer that committed, a pane member, an owned hold
/// and some files in the team directory.
fn busy_team() -> Rig {
    let mut rig = Rig::new();
    let api = rig.add_worktree_member("api", MemberKind::Implementer);
    std::fs::write(api.cwd.join("lib.rs"), "fn a() {}\n").unwrap();
    git(&api.cwd, &["add", "-A"]);
    git(&api.cwd, &["commit", "-q", "-m", "work"]);
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.team_file("reports/api.md", "- abc1234 did it\n");
    rig.team_file("comms/m001.md", "hello\n");
    rig.start_owned_hold();
    rig
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[test]
fn a_full_close_runs_the_fixed_order_and_archives_everything() {
    let mut rig = busy_team();
    let api = rig.member("api");
    let hold = rig.team().sleep_hold.unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());

    assert_eq!(step_names(&report), ORDER, "{report:?}");
    assert!(report.steps.iter().all(|s| s.ok), "{report:?}");
    assert_eq!(report.stopped_at, None);
    assert!(report.refusals.is_empty());

    // Worktree removed, branch kept and named, unpushed work reported.
    assert!(!api.cwd.exists());
    assert_eq!(
        git(&rig.repo, &["branch", "--list", "br-api"]).trim(),
        "br-api"
    );
    assert_eq!(report.branches_kept, ["br-api"]);
    assert_eq!(report.unpushed.len(), 1, "{:?}", report.unpushed);
    assert!(
        report.unpushed.first().unwrap().contains("1 commit(s)"),
        "{:?}",
        report.unpushed
    );
    // Places closed through the host, the hold released, the state gone.
    for role in ["api", "scout"] {
        let place = rig.member(role).place.unwrap();
        let state = rig.host.state(&place, std::time::Duration::ZERO);
        assert!(state.is_err(), "{role}'s place is still open: {state:?}");
    }
    assert!(
        rig.platform
            .inner
            .hold_events()
            .contains(&HoldEvent::Released {
                hold: hold.pid,
                outcome: ReleaseOutcome::Released
            })
    );
    assert!(!rig.team_dir().exists());

    // The archive: one directory, every file, SHA256SUMS that checks out, and
    // a database that already holds team.closed.
    let dir = report.archive.clone().unwrap();
    assert_eq!(entries(&rig.archive_root()), std::slice::from_ref(&dir));
    assert_eq!(
        std::fs::read_to_string(dir.join("reports/api.md")).unwrap(),
        "- abc1234 did it\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("comms/m001.md")).unwrap(),
        "hello\n"
    );
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).unwrap();
    let mut listed: Vec<&str> = Vec::new();
    for line in sums.lines() {
        let (hash, path) = line.split_once("  ").unwrap();
        assert_eq!(sha(&std::fs::read(dir.join(path)).unwrap()), hash, "{path}");
        listed.push(path);
    }
    assert_eq!(listed, ["comms/m001.md", "reports/api.md", "team.db"]);
    let db = Store::open_read_only_at(&dir.join("team.db")).unwrap();
    assert_eq!(db.integrity_check().unwrap(), "ok");
    let team = db.read(|tx| tx.team()).unwrap().unwrap();
    assert_eq!(team.state, TeamState::Closed);
    let closed = db
        .read(|tx| Ok(tx.events_of_type(EventType::TeamClosed)?.items))
        .unwrap();
    assert_eq!(closed.len(), 1, "the final backup holds team.closed");
    let steps = db
        .read(|tx| Ok(tx.events_of_type(EventType::CloseStep)?.items))
        .unwrap();
    assert!(
        steps.len() >= 6,
        "the close steps are in the archive: {}",
        steps.len()
    );
}

#[test]
fn closes_member_left_creating() {
    // A launch that crashed after the agent started leaves a `creating` row
    // with its place recorded. Close finds the place and closes it.
    let mut rig = Rig::new();
    let place = rig.host.add_place(
        "tw_alpha_ghost",
        PlaceState::Idle,
        teamwire_core::Occupant {
            session: None,
            pids: Vec::new(),
        },
    );
    let role = Role::parse("ghost").unwrap();
    let now = rig.clock.now();
    rig.store
        .write(|tx| {
            tx.insert_member(
                now,
                &teamwire_store::NewMember {
                    role: role.clone(),
                    title: "ghost".to_owned(),
                    kind: MemberKind::Investigator,
                    mode: teamwire_core::Mode::Pane,
                    harness: teamwire_core::HarnessId::from_static("claude-code"),
                    host: teamwire_core::HostId::from_static("herdr"),
                    cwd: rig.work.path().to_path_buf(),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: Vec::new(),
                    deadline_s: 1800,
                },
            )?;
            tx.set_member_place(&role, &place)
        })
        .unwrap();
    assert_eq!(
        rig.member("ghost").state,
        teamwire_core::MemberState::Creating
    );
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(
        rig.host.state(&place, std::time::Duration::ZERO).is_err(),
        "the place was closed"
    );
    assert!(!rig.team_dir().exists());
}

#[test]
fn busy_recheck_before_each_destructive_step() {
    // A member turns busy, or its worktree turns dirty, after the first
    // checks and before a destructive step. The close stops at that step,
    // and everything is still there.
    for (step, dirty) in [
        ("remove_worktrees", false),
        ("remove_worktrees", true),
        ("close_places", false),
    ] {
        let mut rig = busy_team();
        let api = rig.member("api");
        let place = rig.place_of("api");
        let host = rig.host.clone();
        let file = api.cwd.join("late.txt");
        let report = close_observed(
            &mut rig.ctx(),
            CloseTarget::Team,
            CloseOpts::default(),
            &|now| {
                if now == step {
                    if dirty {
                        std::fs::write(&file, "typed just now\n").unwrap();
                    } else {
                        host.set_state(&place, PlaceState::Working);
                    }
                }
            },
        );
        let label = format!("{step} dirty={dirty}: {report:?}");
        assert_eq!(report.stopped_at.as_deref(), Some(step), "{label}");
        let failed = report.steps.last().unwrap();
        assert!(
            !failed.ok && failed.detail.contains("changed under the lock"),
            "{label}"
        );
        // State intact, and the lock given back so the close can run again.
        assert_eq!(rig.team().state, TeamState::Open, "{label}");
        assert!(rig.team_dir().join("team.db").is_file(), "{label}");
        assert!(rig.team_dir().join("reports/api.md").is_file(), "{label}");
        assert!(
            host.state(&place, std::time::Duration::ZERO).is_ok(),
            "{label}"
        );
        assert!(
            rig.platform
                .inner
                .hold_events()
                .iter()
                .all(|e| !matches!(e, HoldEvent::Released { .. })),
            "{label}"
        );
        if step == "remove_worktrees" {
            assert!(api.cwd.is_dir(), "{label}");
        }
        let closed = rig.events(EventType::TeamClosed);
        assert_eq!(
            closed.len(),
            1,
            "the failed close ends with team.closed outcome=failed"
        );
        let data = &closed.first().unwrap().data;
        assert_eq!(data["outcome"], "failed", "{label}");
        assert_eq!(data["failed_step"], step, "{label}");
    }
}

#[test]
fn a_place_that_will_not_close_stops_the_close_and_a_rerun_finishes_it() {
    let mut rig = busy_team();
    rig.host.inject(
        "close",
        Fault::Fail(HostError::new(HostErrorCode::Other, "herdr refused")),
        1,
    );
    let first = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(
        first.stopped_at.as_deref(),
        Some("close_places"),
        "{first:?}"
    );
    assert!(first.steps.last().unwrap().detail.contains("herdr refused"));
    assert_eq!(rig.team().state, TeamState::Open);
    assert!(rig.team_dir().join("team.db").is_file());
    assert!(
        rig.platform
            .inner
            .hold_events()
            .iter()
            .all(|e| !matches!(e, HoldEvent::Released { .. }))
    );
    let first_archive = first.archive.clone().unwrap();
    assert!(
        first_archive.join("team.db").is_file(),
        "the archive that was taken stays"
    );

    // The same second, so the second archive needs another name.
    let second = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(second.stopped_at, None, "{second:?}");
    let second_archive = second.archive.clone().unwrap();
    assert_ne!(first_archive, second_archive);
    assert!(
        first_archive.join("team.db").is_file(),
        "an earlier archive is never overwritten"
    );
    assert!(!rig.team_dir().exists());
}

#[test]
fn hold_released_before_state_deleted() {
    let mut rig = busy_team();
    let hold = rig.team().sleep_hold.unwrap();
    let team_dir = rig.team_dir();
    let seen: RefCell<Option<(bool, bool, Option<HoldInfo>)>> = RefCell::new(None);
    let released = |events: Vec<HoldEvent>| {
        events.iter().any(|e| {
            matches!(
                e,
                HoldEvent::Released {
                    outcome: ReleaseOutcome::Released,
                    ..
                }
            )
        })
    };
    let platform = rig.platform.clone();
    let store_path = team_dir.join("team.db");
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            if step == "delete_state" {
                let record = Store::open_read_only_at(&store_path)
                    .unwrap()
                    .read(|tx| tx.team())
                    .unwrap()
                    .unwrap()
                    .sleep_hold;
                *seen.borrow_mut() = Some((
                    released(platform.inner.hold_events()),
                    team_dir.is_dir(),
                    record,
                ));
            }
        },
    );
    assert_eq!(report.stopped_at, None, "{report:?}");
    // When the state was about to be deleted, the hold was already gone, the
    // state was still there, and its record no longer named a hold.
    assert_eq!(seen.into_inner(), Some((true, true, None)));
    let events = rig.platform.inner.hold_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, HoldEvent::Released { .. }))
            .count(),
        1
    );
    let _ = hold;
}

#[test]
fn a_crash_between_the_hold_and_the_delete_leaves_no_running_hold_and_a_rerun_continues() {
    let mut rig = busy_team();
    let teams_dir = rig.home.path().join("teams");
    // Deleting the team directory fails: the parent is read-only. This stands
    // for a crash after the hold was released.
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            if step == "delete_state" {
                set_mode(&teams_dir, 0o555);
            }
        },
    );
    set_mode(&teams_dir, 0o755);
    assert_eq!(
        report.stopped_at.as_deref(),
        Some("delete_state"),
        "{report:?}"
    );
    assert_eq!(rig.team().state, TeamState::Closed);
    assert!(
        rig.team().sleep_hold.is_none(),
        "the record says the hold is gone"
    );
    assert!(rig.team_dir().is_dir());

    // Re-running finds the hold gone and finishes; no second release.
    let again = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(again.stopped_at, None, "{again:?}");
    assert!(!rig.team_dir().exists());
    let events = rig.platform.inner.hold_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, HoldEvent::Released { .. }))
            .count(),
        1
    );
    assert!(again.archive.unwrap().join("team.db").is_file());
}

#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn set_mode(path: &std::path::Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn a_hold_the_team_did_not_start_or_another_team_needs_is_left_running() {
    // Not ours: never released.
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.platform
        .inner
        .add_external_hold(Pid::new(rig::LEAD_PID), Pid::new(777));
    rig.record_hold(HoldInfo {
        pid: Pid::new(777),
        owned: false,
    });
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(rig.platform.inner.hold_events().is_empty());

    // Ours, but another open team on the same lead records the same hold.
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let hold = rig.start_owned_hold();
    let other = teamwire_core::TeamName::parse("beta").unwrap();
    let mut other_store = Store::open(rig.home.path(), &other).unwrap();
    let now = rig.clock.now();
    other_store
        .write(|tx| {
            tx.create_team(
                now,
                &teamwire_store::NewTeam {
                    name: other.clone(),
                    title: "Beta".to_owned(),
                    lead_harness: teamwire_core::HarnessId::from_static("claude-code"),
                    lead_host: teamwire_core::HostId::from_static("herdr"),
                    lead_route: teamwire_core::LeadRoute::Sink,
                    lead_session: Some((
                        teamwire_core::SessionId::parse("lead-1").unwrap(),
                        teamwire_core::SessionKind::Interactive,
                    )),
                },
            )?;
            tx.set_sleep_hold(Some(&hold))
        })
        .unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    let release = report
        .steps
        .iter()
        .find(|s| s.step == "release_hold")
        .unwrap();
    assert!(
        release.detail.contains("team beta still records it"),
        "{release:?}"
    );
    assert!(
        rig.platform
            .inner
            .hold_events()
            .iter()
            .all(|e| !matches!(e, HoldEvent::Released { .. })),
        "the other team still needs the hold"
    );

    // Beta is still open with the hold recorded, so alpha's close left it.
    // When beta closes and alpha is closed too, the last team releases it.
    let (registry, platform, clock, config) =
        (&rig.registry, &rig.platform, &rig.clock, &rig.config);
    let home = rig.home.path().to_path_buf();
    let last = close(
        &mut Ctx {
            home: &home,
            store: &mut other_store,
            registry,
            platform: &**platform,
            clock,
            config,
        },
        CloseTarget::Team,
        CloseOpts::default(),
    );
    assert_eq!(last.stopped_at, None, "{last:?}");
    let release = last
        .steps
        .iter()
        .find(|s| s.step == "release_hold")
        .unwrap();
    assert!(
        release.detail.starts_with("released sleep hold"),
        "{release:?}"
    );
    let released: Vec<_> = rig
        .platform
        .inner
        .hold_events()
        .into_iter()
        .filter(|e| {
            matches!(
                e,
                HoldEvent::Released {
                    outcome: ReleaseOutcome::Released,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(released.len(), 1);
}

#[test]
fn stale_close_lock_cleared_and_rerun() {
    let mut rig = Rig::new();
    rig.add_pane_member("scout", MemberKind::Investigator);
    let now = rig.clock.now();
    let dead = Pid::new(4_000_001);
    rig.store.write(|tx| tx.begin_close(now, dead)).unwrap();

    // The lock of a crashed close: a close refuses and says how to go on.
    let refused = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(asks(&refused.refusals, "--clear-stale-lock"), "{refused:?}");
    assert_eq!(rig.team().state, TeamState::Closing);

    // A lock whose owner still runs is never cleared.
    rig.platform.inner.set_alive(dead, true);
    let live = close(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts {
            clear_stale_lock: true,
            ..CloseOpts::default()
        },
    );
    assert!(asks(&live.refusals, "is running"), "{live:?}");
    assert_eq!(rig.team().state, TeamState::Closing);
    let live_close = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        asks(&live_close.refusals, "already running"),
        "{live_close:?}"
    );

    // A dead owner: the flag clears the lock and does nothing else.
    rig.platform.inner.set_alive(dead, false);
    let cleared = close(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts {
            clear_stale_lock: true,
            ..CloseOpts::default()
        },
    );
    assert_eq!(cleared.stopped_at, None, "{cleared:?}");
    assert_eq!(step_names(&cleared), ["clear_stale_lock"]);
    assert_eq!(rig.team().state, TeamState::Open);
    assert!(rig.team().closing_pid.is_none());
    assert!(
        rig.team_dir().join("team.db").is_file(),
        "nothing else was done"
    );
    assert!(!rig.archive_root().exists());

    // Then the close runs again from the first step.
    let rerun = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(step_names(&rerun), ORDER, "{rerun:?}");
    assert_eq!(rerun.stopped_at, None);
    assert!(!rig.team_dir().exists());

    // With no lock at all, the flag is a no-op that says so.
    let mut fresh = Rig::new();
    let none = close(
        &mut fresh.ctx(),
        CloseTarget::Team,
        CloseOpts {
            clear_stale_lock: true,
            ..CloseOpts::default()
        },
    );
    assert_eq!(none.stopped_at, None);
    assert!(
        none.steps.first().unwrap().detail.contains("no close lock"),
        "{none:?}"
    );
}

#[test]
fn reprompt_and_add_refused_during_close() {
    // Launch --add, reprompt and removal each read the team state inside their
    // own write transaction. While a close runs, that state is `closing`, and
    // a second close or a removal is refused.
    let mut rig = busy_team();
    let home = rig.home.path().to_path_buf();
    let team = teamwire_core::TeamName::parse(rig::TEAM).unwrap();
    let seen: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let (registry, clock, config) = (&rig.registry, &rig.clock, &rig.config);
    let platform: &dyn teamwire_core::Platform = &*rig.platform;
    let mut ctx = Ctx {
        home: &home,
        store: &mut rig.store,
        registry,
        platform,
        clock,
        config,
    };
    let report = close_observed(&mut ctx, CloseTarget::Team, CloseOpts::default(), &|step| {
        if step != "remove_worktrees" {
            return;
        }
        let mut other = Store::open_existing(&home, &team).unwrap();
        // What every service checks inside its own write transaction.
        let state = other
            .write(|tx| Ok(tx.reader().team()?.unwrap().state))
            .unwrap();
        seen.borrow_mut().push(format!("state={state}"));
        // The lock cannot be taken twice.
        let again = other
            .write(|tx| tx.begin_close(clock.now(), Pid::new(1)))
            .unwrap();
        seen.borrow_mut().push(format!("second_lock={again}"));
        // A second close and a removal, through the public calls.
        let mut second = Ctx {
            home: &home,
            store: &mut other,
            registry,
            platform,
            clock,
            config,
        };
        let close_again = close(&mut second, CloseTarget::Team, CloseOpts::default());
        seen.borrow_mut().push(format!(
            "close_refused={}",
            !close_again.refusals.is_empty()
        ));
        let removal = close(
            &mut second,
            CloseTarget::Member(Role::parse("scout").unwrap()),
            CloseOpts::default(),
        );
        seen.borrow_mut()
            .push(format!("removal_refused={}", !removal.refusals.is_empty()));
    });
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(
        seen.into_inner(),
        [
            "state=closing",
            "second_lock=false",
            "close_refused=true",
            "removal_refused=true"
        ]
    );
}

#[test]
fn keep_state_leaves_the_directory_marked_closed_and_a_rerun_can_finish_it() {
    let mut rig = busy_team();
    let opts = CloseOpts {
        keep_state: true,
        ..CloseOpts::default()
    };
    let report = close(&mut rig.ctx(), CloseTarget::Team, opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(rig.team_dir().join("team.db").is_file());
    assert_eq!(rig.team().state, TeamState::Closed);
    let first = report.archive.unwrap();
    assert!(first.join("SHA256SUMS").is_file());

    // A closed team with its directory still there: close again removes it,
    // after a fresh archive that is verified the same way.
    let again = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(again.stopped_at, None, "{again:?}");
    assert_eq!(
        step_names(&again),
        ["archive", "verify_archive", "delete_state"]
    );
    assert!(!rig.team_dir().exists());
    assert_ne!(again.archive.unwrap(), first);

    // A closed team with no directory has nothing left to close.
    let last = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(asks(&last.refusals, "already closed"), "{last:?}");
}

#[test]
fn the_archive_goes_where_asked_and_never_over_an_existing_path() {
    let mut rig = busy_team();
    let elsewhere: PathBuf = rig.work.path().join("out");
    let opts = CloseOpts {
        archive_to: Some(elsewhere.clone()),
        ..CloseOpts::default()
    };
    let plan = teamwire_close::plan_close_with(&rig.ctx(), CloseTarget::Team, &opts);
    let dry = plan.steps.iter().find(|s| s.step == "archive").unwrap();
    assert!(
        dry.detail.contains(&elsewhere.display().to_string()),
        "{dry:?}"
    );
    assert!(!elsewhere.exists(), "a plan creates nothing");

    // The date's name is taken by something else: the archive takes another
    // name and the earlier path is untouched.
    let date_dir = {
        let probe = teamwire_close::plan_close_with(&rig.ctx(), CloseTarget::Team, &opts);
        let text = &probe
            .steps
            .iter()
            .find(|s| s.step == "archive")
            .unwrap()
            .detail;
        PathBuf::from(text.rsplit(": ").next().unwrap())
    };
    std::fs::create_dir_all(&date_dir).unwrap();
    std::fs::write(date_dir.join("mine.txt"), "keep me\n").unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, opts);
    assert_eq!(report.stopped_at, None, "{report:?}");
    let used = report.archive.unwrap();
    assert_ne!(used, date_dir);
    assert!(used.starts_with(&elsewhere));
    assert_eq!(
        std::fs::read_to_string(date_dir.join("mine.txt")).unwrap(),
        "keep me\n"
    );
    assert!(!date_dir.join("team.db").exists());
}

#[test]
fn a_private_file_stays_private_in_the_archive() {
    let mut rig = busy_team();
    rig.team_file("members/api/settings.json", "{\"secret\": true}\n");
    set_mode(&rig.team_dir().join("members/api/settings.json"), 0o600);
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    let mode = std::fs::metadata(report.archive.unwrap().join("members/api/settings.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
