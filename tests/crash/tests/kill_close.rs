//! `kill -9` of `teamwire close` after the archive is taken and before the
//! state directory is deleted, then `close` again
//! (`docs/test-strategy.md` section 3, `docs/failure-modes.md` section 2).
#![expect(
    clippy::disallowed_methods,
    reason = "the test measures real elapsed time to place its kill points"
)]
#![expect(
    clippy::print_stderr,
    reason = "the kill points and the counts are printed so a failing run can be read"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::time::{Duration, Instant};

use rig::{
    Identity, Rig, archive_verifies, archives, assert_closed_for_good, assert_integrity,
    close_until_done, launch_scout, stop_agents, team_name,
};
use teamwire_core::TeamState;
use teamwire_store::Store;

fn agents(rig: &Rig) -> usize {
    let out = rig.herdr.run(&["agent", "list"]).unwrap();
    out.result()
        .and_then(|r| r.get("agents")?.as_array().map(Vec::len))
        .unwrap_or(0)
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn team_state(rig: &Rig) -> Option<TeamState> {
    let store = Store::open_read_only(rig.home.path(), &team_name()).ok()?;
    let team = store.read(|tx| tx.team()).ok()??;
    Some(team.state)
}

#[test]
fn kill9_during_close_after_the_archive_is_cleared_and_rerun() {
    let mut rig = Rig::bare();
    let work = tempfile::tempdir().unwrap();
    launch_scout(&rig, work.path());
    // Closing the member's place comes after the archive and its check.
    let signal = rig.pause_after("pane close");
    let mut running = rig.start(&["close", "alpha"], &Identity::Nobody);
    Rig::wait_for(&signal, &mut running);
    running.kill9();
    running.finish();
    rig.path_prefix = None;

    // The archive is whole, and the team is still there, locked.
    let taken = archives(rig.home.path());
    assert_eq!(taken.len(), 1, "{taken:?}");
    assert!(archive_verifies(&taken[0]));
    assert_integrity(&Store::open_existing(rig.home.path(), &team_name()).unwrap());
    assert_eq!(team_state(&rig), Some(TeamState::Closing));

    // Close refuses until the dead owner's lock is cleared, then finishes.
    let refused = rig.run(&["close", "alpha"], &Identity::Nobody);
    assert_eq!(refused.code, Some(1), "{refused:?}");
    assert_eq!(close_until_done(&rig), 2);
    assert_eq!(agents(&rig), 0, "the agent was left running");
    assert_closed_for_good(&rig);
    stop_agents(&rig);
}

const MIN_RUNS: usize = 8;
const MAX_RUNS: usize = 40;
/// Kills that must land between `closed` and the deletion.
const NEEDED: usize = 2;

#[test]
fn kill9_between_closed_and_state_deleted_finishes_on_rerun() {
    let mut rng = rig::Rng::from_env("kill9_between_closed_and_state_deleted");
    let work = tempfile::tempdir().unwrap();
    let mut caught_closed = 0;
    let mut runs_made = 0;
    for i in 0..MAX_RUNS {
        // The window is some 20 ms wide and a slow machine moves it, so the
        // test goes on past MIN_RUNS until enough kills landed in it.
        if i >= MIN_RUNS && caught_closed >= NEEDED {
            break;
        }
        runs_made += 1;
        let rig = Rig::bare();
        launch_scout(&rig, work.path());
        let mut running = rig.start(&["close", "alpha"], &Identity::Nobody);
        // Wait for the team to be marked closed: the archive exists, the
        // hold is released, and only the last archive refresh and the
        // deletion are left.
        let waited = Instant::now();
        let mut seen = false;
        while waited.elapsed() < Duration::from_secs(20) {
            if team_state(&rig) == Some(TeamState::Closed) {
                seen = true;
                break;
            }
            if !rig.home.path().join("teams/alpha").exists() {
                break;
            }
            std::thread::sleep(Duration::from_micros(300));
        }
        if !seen {
            eprintln!(
                "the team was never seen closed: state {:?} out {:?} {:?}",
                team_state(&rig),
                running.stderr_so_far(),
                agents(&rig)
            );
        }
        let extra = rng.up_to(Duration::from_millis(25));
        let begun = Instant::now();
        while begun.elapsed() < extra {
            std::hint::spin_loop();
        }
        running.kill9();
        running.finish();

        let state_left = rig.home.path().join("teams/alpha").exists();
        // A kill that came after the deletion missed the window it names.
        caught_closed += usize::from(seen && state_left);
        if state_left {
            let store = Store::open_existing(rig.home.path(), &team_name()).unwrap();
            assert_integrity(&store);
            drop(store);
        }
        let taken = archives(rig.home.path());
        assert!(!taken.is_empty(), "run {i}: no archive");
        let runs = if state_left {
            close_until_done(&rig)
        } else {
            0
        };
        eprintln!(
            "run {i}: closed seen {seen}, killed +{extra:?}, state left {state_left}, reruns {runs}"
        );
        assert_eq!(agents(&rig), 0, "run {i}: the agent was left running");
        assert_closed_for_good(&rig);
        stop_agents(&rig);
    }
    eprintln!("{caught_closed} of {runs_made} runs killed close between closed and deleted");
    assert!(
        caught_closed >= NEEDED,
        "only {caught_closed} of {runs_made} runs killed close between closed and deleted; the window between closed and deleted was missed"
    );
}
