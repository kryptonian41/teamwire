//! `lead_hand_off` (ADR 0051) against the fake host and the fake harness:
//! the order of the host calls and the store writes, both ways a new lead
//! session is recorded, each refusal, and each failure after the new pane
//! opened.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "helper functions in an integration-test file unwrap, index and panic; a failure there fails the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes the handoff files the old lead would, not through the platform"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::Rig;
use teamwire_core::harness::WatchStart;
use teamwire_core::{
    Clock, Config, HarnessCaps, HarnessError, HostCaps, HostError, HostErrorCode, Liveness,
    Occupant, Pid, Place, PlaceState, SessionId, SessionKind, SplitRequest, TeamName,
};
use teamwire_fakes::{FakeHarness, FakeHost, Fault};
use teamwire_launch::{
    HandoffError, LeadCaller, LeadHandoffCtx, LeadHandoffOpts, LeadHandoffWaits, Said,
    lead_hand_off,
};
use teamwire_store::{NewTeam, Store};

const OLD: &str = "lead-session";
const OLD_PID: u32 = 500;
const NEW_PID: u32 = 600;

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

/// An open team led by the old session, with a title.
fn team(rig: &Rig, name: &str, title: &str, away: bool) {
    let team = TeamName::parse(name).unwrap();
    let mut store = Store::open(rig.home.path(), &team).unwrap();
    let at = rig.clock.now();
    let new = NewTeam {
        name: team,
        title: title.to_owned(),
        lead_harness: teamwire_core::HarnessId::from_static(FakeHarness::ID),
        lead_host: teamwire_core::HostId::from_static(FakeHost::ID),
        lead_route: teamwire_core::LeadRoute::Sink,
        lead_session: Some((sid(OLD), SessionKind::Interactive)),
    };
    store
        .write(|tx| {
            if away {
                tx.create_afk_team(at, &new, at, &serde_json::json!({}))
                    .map(|_| ())
            } else {
                tx.create_team(at, &new).map(|_| ())
            }
        })
        .unwrap();
}

/// The old lead runs in a pane of its own, with its handoff document.
fn old_lead(rig: &Rig) -> Place {
    rig.harness
        .add_session(sid(OLD), vec![Pid::new(OLD_PID)], alive());
    rig.harness.set_session_cwd(sid(OLD), rig.work.path());
    rig.platform.inner.set_alive(Pid::new(OLD_PID), true);
    rig.platform.inner.set_args(
        Pid::new(OLD_PID),
        &["fake-claude", "--resume", OLD, "--keep", "--drop-me"],
    );
    let place = rig.host.add_place(
        "",
        PlaceState::Idle,
        Occupant {
            session: Some(sid(OLD)),
            pids: vec![Pid::new(OLD_PID)],
        },
    );
    rig.platform
        .inner
        .set_env(Pid::new(OLD_PID), "FAKE_PLACE_ID", &place.id);
    write_document(rig.home.path());
    place
}

fn alive() -> Liveness {
    Liveness::Alive(PlaceState::Working)
}

fn write_document(home: &Path) {
    let dir = home.join("handoffs").join(OLD);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("handoff.md"), "# handoff\n").unwrap();
}

/// Sets the modification time of `path` to `secs` after the epoch.
fn modified_at(path: &Path, secs: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

/// The new pane's command starts the session its argv names, as a real
/// harness does once the pane's shell runs it.
fn new_session_starts(rig: &Rig) {
    new_session_starts_then(rig, || {});
}

/// `new_session_starts`, then `after` once the session runs.
fn new_session_starts_then(rig: &Rig, after: impl Fn() + Send + Sync + 'static) {
    let harness = Arc::clone(&rig.harness);
    let platform = Arc::clone(&rig.platform);
    rig.host
        .on_split(move |host: &FakeHost, place: &Place, req: &SplitRequest| {
            let at = req.argv.iter().position(|a| a == "--session-id").unwrap();
            let new = sid(&req.argv[at + 1]);
            harness.add_session(new.clone(), vec![Pid::new(NEW_PID)], alive());
            platform.inner.set_alive(Pid::new(NEW_PID), true);
            platform
                .inner
                .set_env(Pid::new(NEW_PID), "FAKE_PLACE_ID", &place.id);
            host.set_occupant(
                place,
                Occupant {
                    session: Some(new),
                    pids: vec![Pid::new(NEW_PID)],
                },
            );
            after();
        });
}

fn waits() -> LeadHandoffWaits {
    LeadHandoffWaits {
        pause: Duration::from_millis(1),
        prove: Duration::from_millis(20),
        adopt: Duration::from_millis(20),
    }
}

fn run_with(rig: &Rig, is_member: bool, dry_run: bool) -> (Vec<Said>, Result<(), HandoffError>) {
    let ctx = LeadHandoffCtx {
        home: rig.home.path(),
        registry: &rig.registry,
        platform: &*rig.platform,
        clock: &rig.clock as &dyn Clock,
        config: &rig.config,
    };
    let env = |_: &str| None;
    let caller = LeadCaller {
        env: &env,
        is_member,
    };
    let opts = LeadHandoffOpts {
        home_env: Some(rig.home.path().to_path_buf()),
        dry_run,
        waits: waits(),
    };
    let mut said = Vec::new();
    let result = lead_hand_off(&ctx, &caller, &opts, &mut |s| said.push(s));
    (said, result)
}

fn run(rig: &Rig) -> (Vec<Said>, Result<(), HandoffError>) {
    run_with(rig, false, false)
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn lead_of(rig: &Rig, name: &str) -> Option<SessionId> {
    let store = Store::open_read_only(rig.home.path(), &TeamName::parse(name).unwrap()).unwrap();
    store.read(|tx| tx.team()).unwrap().unwrap().lead_session
}

/// The host calls that change something, in order: `split_run <beside>
/// <label>` and `close_pane <place>`.
fn host_writes(rig: &Rig) -> Vec<String> {
    rig.host
        .log()
        .calls()
        .into_iter()
        .filter(|c| matches!(c.name, "split_run" | "close_pane"))
        .map(|c| format!("{} {}", c.name, c.detail))
        .collect()
}

fn text(said: &[Said]) -> String {
    said.iter()
        .map(|s| match s {
            Said::Line(t) | Said::Warning(t) => t.as_str(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The single pane `split_run` made, and its request.
fn split_pane(rig: &Rig, old: &Place) -> (Place, SplitRequest) {
    let made: Vec<Place> = (1..=4)
        .map(|n| Place {
            host: old.host.clone(),
            id: format!("p{n}"),
        })
        .filter(|p| rig.host.split_request_of(p).is_some())
        .collect();
    assert_eq!(made.len(), 1, "{made:?}");
    let req = rig.host.split_request_of(&made[0]).unwrap();
    (made[0].clone(), req)
}

#[test]
fn a_handoff_starts_the_new_lead_beside_the_old_moves_every_team_and_closes_the_old_pane() {
    let rig = Rig::new();
    team(&rig, "beta", "Second team", false);
    team(&rig, "alpha", "Fix the prompts", false);
    let old = old_lead(&rig);
    new_session_starts(&rig);
    rig.harness.set_lead_start_drops(&["--drop-me"]);

    let (said, result) = run(&rig);
    assert_eq!(result, Ok(()), "{}", text(&said));

    let (new_place, req) = split_pane(&rig, &old);
    assert_eq!(
        host_writes(&rig),
        [
            format!("split_run {} lead · Fix the prompts +1", old.id),
            format!("close_pane {}", old.id),
        ]
    );
    assert_eq!(req.cwd, rig.work.path());
    assert_eq!(
        req.env,
        [(
            "TEAMWIRE_HOME".to_owned(),
            rig.home.path().display().to_string()
        )]
    );
    assert_eq!(req.argv[..2], ["fake-claude", "--session-id"]);
    let new = sid(&req.argv[2]);
    assert_ne!(new, sid(OLD));
    // The harness got the old lead's arguments, and left out what it drops.
    assert_eq!(
        rig.harness.lead_starts()[0].old_args,
        ["fake-claude", "--resume", OLD, "--keep", "--drop-me"]
    );
    assert_eq!(req.argv[3..req.argv.len() - 1], ["--resume", OLD, "--keep"]);
    assert_eq!(lead_of(&rig, "alpha"), Some(new.clone()));
    assert_eq!(lead_of(&rig, "beta"), Some(new.clone()));
    let all = text(&said);
    assert!(
        said.contains(&Said::Warning(
            "warning: the old lead's flag --drop-me is not passed to the new lead".to_owned()
        )),
        "{all}"
    );
    assert!(
        all.contains(&format!("session {new} runs in pane {}", new_place.id)),
        "{all}"
    );
    assert!(
        all.contains("session lead-session is no longer the lead"),
        "{all}"
    );
    // The sleep hold now waits on the new lead's process.
    let started: Vec<_> = rig
        .platform
        .inner
        .log()
        .calls()
        .into_iter()
        .filter(|c| c.name == "hold_start")
        .map(|c| c.detail)
        .collect();
    assert!(
        started.iter().all(|d| d.contains(&NEW_PID.to_string())),
        "{started:?}"
    );
    assert!(!started.is_empty(), "{all}");
}

#[test]
fn the_first_prompt_points_at_the_document_and_names_each_team() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    team(&rig, "beta", "Away", true);
    old_lead(&rig);
    new_session_starts(&rig);
    let (said, result) = run(&rig);
    assert_eq!(result, Ok(()), "{}", text(&said));
    let (_, req) = split_pane(
        &rig,
        &Place {
            host: teamwire_core::HostId::from_static(FakeHost::ID),
            id: String::new(),
        },
    );
    let document = rig
        .home
        .path()
        .join("handoffs")
        .join(OLD)
        .join("handoff.md");
    assert_eq!(
        req.argv.last().unwrap(),
        &format!(
            "You are a teamwire lead. You take over from lead session lead-session, which handed \
             off because its context was full. You now lead these teams: alpha, beta. Load the \
             fake teamwire skill. Team beta is an away run: load the teamwire:afk skill too, and \
             follow its section on taking over a run. Then read {} in full. It assumes no prior \
             conversation. Then run `teamwire status alpha`, `teamwire status beta` and \
             `teamwire lead --missed`. Start a watch for each team that has working members \
             (`teamwire watch alpha`; `teamwire watch beta`). If the watch is refused because \
             another watch holds the team, wait 15 seconds and try once more. Then carry on from \
             the document's Next section. Do not plan the work again, and do not ask the user \
             what the document already answers.",
            document.display()
        )
    );
}

/// A harness that picks its own session id, like Codex: the new lead
/// adopts each team itself, and the command waits for that.
/// The new lead picks its own session id, and adopts the teams in `adopts`.
fn picks_its_own_id(rig: &Rig, adopts: &'static [&'static str]) {
    rig.harness.set_caps(HarnessCaps {
        choose_session_id: false,
        watch_start: WatchStart::BackgroundCommand,
        ..FakeHarness::caps()
    });
    let home = rig.home.path().to_path_buf();
    let harness = Arc::clone(&rig.harness);
    let platform = Arc::clone(&rig.platform);
    rig.host
        .on_split(move |host: &FakeHost, place: &Place, _req: &SplitRequest| {
            let new = sid("picked-by-the-harness");
            harness.add_session(new.clone(), vec![Pid::new(NEW_PID)], alive());
            platform.inner.set_alive(Pid::new(NEW_PID), true);
            platform
                .inner
                .set_env(Pid::new(NEW_PID), "FAKE_PLACE_ID", &place.id);
            host.set_occupant(
                place,
                Occupant {
                    session: Some(new.clone()),
                    pids: vec![Pid::new(NEW_PID)],
                },
            );
            for name in adopts {
                let mut store =
                    Store::open_existing(&home, &TeamName::parse(name).unwrap()).unwrap();
                store
                    .write(|tx| {
                        tx.adopt_lead(
                            teamwire_core::Timestamp::from_unix_millis(1_800_000_000_000).unwrap(),
                            &new,
                            SessionKind::Interactive,
                        )
                        .map(|_| ())
                    })
                    .unwrap();
            }
        });
}

#[test]
fn a_lead_that_picks_its_own_id_adopts_each_team_and_the_command_waits_for_it() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    team(&rig, "beta", "Second team", false);
    let old = old_lead(&rig);
    picks_its_own_id(&rig, &["alpha", "beta"]);

    let (said, result) = run(&rig);
    assert_eq!(result, Ok(()), "{}", text(&said));

    let (_, req) = split_pane(&rig, &old);
    assert_eq!(rig.harness.lead_starts()[0].new_session, None);
    // A lead that runs its watch in the background is told so.
    assert!(
        req.argv.last().unwrap().contains(
            "(`nohup teamwire watch alpha --deliver >/dev/null 2>&1 &`; `nohup teamwire watch \
             beta --deliver >/dev/null 2>&1 &`)"
        ),
        "{req:?}"
    );
    assert!(
        req.argv.last().unwrap().starts_with(
            "First run `teamwire lead --adopt <team>` for each of these teams: alpha, beta. You \
             are a teamwire lead."
        ),
        "{req:?}"
    );
    assert_eq!(lead_of(&rig, "alpha"), Some(sid("picked-by-the-harness")));
    assert_eq!(lead_of(&rig, "beta"), Some(sid("picked-by-the-harness")));
    assert_eq!(
        host_writes(&rig).last().unwrap(),
        &format!("close_pane {}", old.id)
    );
}

#[test]
fn a_lead_that_never_adopts_leaves_both_panes_open() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    team(&rig, "beta", "Second team", false);
    let old = old_lead(&rig);
    picks_its_own_id(&rig, &[]);

    let (said, result) = run(&rig);
    let Err(HandoffError::Partial(why)) = result else {
        panic!("{result:?} {}", text(&said));
    };
    assert!(why.contains("did not adopt every team"), "{why}");
    assert!(
        why.contains("Still led by this session: alpha, beta"),
        "{why}"
    );
    assert!(
        why.contains("`teamwire lead --adopt alpha`, `teamwire lead --adopt beta`"),
        "{why}"
    );
    assert_eq!(
        host_writes(&rig).len(),
        1,
        "only the split: {:?}",
        host_writes(&rig)
    );
    assert!(host_writes(&rig)[0].starts_with(&format!("split_run {}", old.id)));
}

#[test]
fn a_new_session_that_never_shows_closes_the_new_pane_and_moves_no_team() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    let old = old_lead(&rig);

    let (said, result) = run(&rig);
    let Err(HandoffError::Partial(why)) = result else {
        panic!("{result:?} {}", text(&said));
    };
    let new_place = split_pane_gone(&rig);
    assert_eq!(
        host_writes(&rig),
        [
            format!("split_run {} lead · Fix the prompts", old.id),
            format!("close_pane {new_place}"),
        ]
    );
    assert!(why.contains("was not seen in pane"), "{why}");
    assert!(
        why.contains("Every team still has this session as its lead"),
        "{why}"
    );
    assert_eq!(lead_of(&rig, "alpha"), Some(sid(OLD)));
}

/// The id of the one pane `split_run` made and a close removed.
fn split_pane_gone(rig: &Rig) -> String {
    let closed: Vec<String> = rig
        .host
        .log()
        .calls()
        .into_iter()
        .filter(|c| c.name == "close_pane")
        .map(|c| c.detail)
        .collect();
    assert_eq!(closed.len(), 1, "{closed:?}");
    closed[0].clone()
}

#[test]
fn a_team_that_cannot_move_stops_the_handoff_with_both_panes_open() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    team(&rig, "beta", "Second team", false);
    let old = old_lead(&rig);
    // Another lead adopts beta between the plan and the adoption.
    let home = rig.home.path().to_path_buf();
    new_session_starts_then(&rig, move || {
        let mut store = Store::open_existing(&home, &TeamName::parse("beta").unwrap()).unwrap();
        store
            .write(|tx| {
                tx.adopt_lead(
                    teamwire_core::Timestamp::from_unix_millis(1_800_000_000_000).unwrap(),
                    &sid("someone-else"),
                    SessionKind::Interactive,
                )
                .map(|_| ())
            })
            .unwrap();
    });

    let (said, result) = run(&rig);
    let Err(HandoffError::Partial(why)) = result else {
        panic!("{result:?} {}", text(&said));
    };
    let new = sid(&rig
        .host
        .split_request_of(&split_pane(&rig, &old).0)
        .unwrap()
        .argv[2]);
    assert_eq!(lead_of(&rig, "alpha"), Some(new));
    assert_eq!(lead_of(&rig, "beta"), Some(sid("someone-else")));
    assert!(
        why.starts_with("team beta: it is open and its lead is someone-else"),
        "{why}"
    );
    assert!(
        why.contains("Moved to the new lead: alpha. Still led by this session: beta"),
        "{why}"
    );
    assert!(why.contains("`teamwire lead --adopt beta`"), "{why}");
    assert_eq!(
        host_writes(&rig).len(),
        1,
        "no pane closed: {:?}",
        host_writes(&rig)
    );
    // alpha moved, so its sleep hold now waits on the new lead's process.
    assert!(
        hold_starts(&rig).contains(&NEW_PID.to_string()),
        "{:?} {}",
        hold_starts(&rig),
        text(&said)
    );
}

/// The lead process of each sleep hold started so far.
fn hold_starts(rig: &Rig) -> Vec<String> {
    rig.platform
        .inner
        .log()
        .calls()
        .into_iter()
        .filter(|c| c.name == "hold_start")
        .map(|c| c.detail)
        .collect()
}

#[test]
fn a_lead_that_adopts_some_teams_moves_their_sleep_hold_and_leaves_both_panes_open() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    team(&rig, "beta", "Second team", false);
    old_lead(&rig);
    picks_its_own_id(&rig, &["alpha"]);

    let (said, result) = run(&rig);
    let Err(HandoffError::Partial(why)) = result else {
        panic!("{result:?} {}", text(&said));
    };
    assert!(
        why.contains("Moved to the new lead: alpha. Still led by this session: beta"),
        "{why}"
    );
    assert_eq!(lead_of(&rig, "beta"), Some(sid(OLD)));
    assert_eq!(
        hold_starts(&rig),
        vec![NEW_PID.to_string()],
        "only alpha's hold moves: {}",
        text(&said)
    );
}

#[test]
fn a_dry_run_says_the_plan_and_changes_nothing() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    let old = old_lead(&rig);
    rig.harness.set_lead_start_drops(&["--drop-me"]);

    let (said, result) = run_with(&rig, false, true);
    assert_eq!(result, Ok(()), "{}", text(&said));
    let all = text(&said);
    for want in [
        "teams: alpha".to_owned(),
        format!("old pane: {} (session lead-session)", old.id),
        format!(
            "handoff document: {}",
            rig.home
                .path()
                .join("handoffs/lead-session/handoff.md")
                .display()
        ),
        format!("in {}", rig.work.path().display()),
        "command: fake-claude --session-id ".to_owned(),
        "not passed on: --drop-me".to_owned(),
        "dry run: no pane was opened and no team changed".to_owned(),
    ] {
        assert!(all.contains(&want), "{want} not in:\n{all}");
    }
    assert!(host_writes(&rig).is_empty());
    assert_eq!(lead_of(&rig, "alpha"), Some(sid(OLD)));
}

/// Each refusal before anything changes: the result, and no host write.
fn refused(rig: &Rig, is_member: bool) -> String {
    let (said, result) = run_with(rig, is_member, false);
    let Err(HandoffError::Refused(why)) = result else {
        panic!("{result:?} {}", text(&said));
    };
    assert!(host_writes(rig).is_empty(), "{:?}", host_writes(rig));
    why
}

#[test]
fn a_member_session_is_refused() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    assert!(refused(&rig, true).contains("member's session"));
}

#[test]
fn a_session_that_leads_no_open_team_is_refused() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    rig.harness.set_interactive_caller(sid("not-the-lead"));
    assert!(refused(&rig, false).contains("leads no open team"));
}

#[test]
fn a_background_session_is_refused() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    rig.harness.set_background_caller(sid(OLD));
    assert!(refused(&rig, false).contains("background session"));
}

#[test]
fn a_host_that_cannot_split_is_refused_with_the_command_to_run_by_hand() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    rig.host.set_caps(HostCaps {
        splits_pane: false,
        ..FakeHost::caps()
    });
    let why = refused(&rig, false);
    assert!(why.contains("cannot open a pane beside this one"), "{why}");
    assert!(
        why.contains(&format!("cd {} && fake-claude", rig.work.path().display())),
        "{why}"
    );
    // Nothing records a lead started by hand, so it adopts first.
    assert!(
        why.contains("First run `teamwire lead --adopt <team>`"),
        "{why}"
    );
}

#[test]
fn a_harness_that_cannot_start_a_lead_is_refused() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    rig.harness
        .fail_lead_start(Some(HarnessError::unsupported()));
    assert!(refused(&rig, false).contains("cannot start a new lead"));
}

#[test]
fn a_missing_handoff_document_is_refused() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    std::fs::remove_file(rig.home.path().join("handoffs/lead-session/handoff.md")).unwrap();
    let why = refused(&rig, false);
    assert!(why.contains("teamwire lead --handoff-guide"), "{why}");
}

#[test]
fn an_old_lead_that_cannot_be_located_is_refused_with_the_reason_code() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    rig.harness.add_session(sid(OLD), Vec::new(), alive());
    let why = refused(&rig, false);
    assert!(why.contains("cannot be located (no_live_process)"), "{why}");
}

#[test]
fn a_document_older_than_the_note_gives_a_warning_and_the_handoff_goes_on() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    new_session_starts(&rig);
    let dir = rig.home.path().join("handoffs").join(OLD);
    std::fs::write(dir.join("noted"), "").unwrap();
    // Whole seconds apart, so a file system that keeps whole-second times
    // still orders them.
    modified_at(&dir.join("handoff.md"), 1_000);
    modified_at(&dir.join("noted"), 1_010);
    let (said, result) = run(&rig);
    assert_eq!(result, Ok(()), "{}", text(&said));
    assert!(
        said.iter()
            .any(|s| matches!(s, Said::Warning(w) if w.contains("is older than the note"))),
        "{}",
        text(&said)
    );
}

#[test]
fn a_team_store_that_cannot_be_read_stops_the_handoff_as_a_store_error() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    let broken = rig.home.path().join("teams/broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("team.db"), "not a database").unwrap();
    let (said, result) = run(&rig);
    assert!(
        matches!(result, Err(HandoffError::Store(_))),
        "{result:?} {}",
        text(&said)
    );
    assert!(host_writes(&rig).is_empty());
}

#[test]
fn an_old_pane_that_does_not_close_gives_a_warning_after_the_teams_moved() {
    let rig = Rig::new();
    team(&rig, "alpha", "Fix the prompts", false);
    old_lead(&rig);
    new_session_starts(&rig);
    rig.host.inject_always(
        "close_pane",
        Fault::Fail(HostError::new(HostErrorCode::Other, "pane busy")),
    );
    let (said, result) = run(&rig);
    assert_eq!(result, Ok(()), "{}", text(&said));
    assert_ne!(lead_of(&rig, "alpha"), Some(sid(OLD)));
    assert!(
        said.iter().any(|s| matches!(s, Said::Warning(w) if w.contains("could not be closed (other: pane busy)") && w.contains("End this turn now"))),
        "{}",
        text(&said)
    );
}

#[test]
fn the_proof_wait_ends_before_a_two_minute_command_limit() {
    // Claude Code's Bash tool stops a command after 120 s by default, so the
    // proof gives up by 90 s and still closes the new pane and says why.
    let config = Config::default();
    assert_eq!(config.spawn_timeout_s, 150);
    let waits = LeadHandoffWaits::of(&config);
    assert_eq!(waits.prove, Duration::from_secs(90));
    assert_eq!(waits.adopt, Duration::from_secs(180));
    let lower = Config {
        spawn_timeout_s: 60,
        ..Config::default()
    };
    assert_eq!(LeadHandoffWaits::of(&lower).prove, Duration::from_secs(60));
}
