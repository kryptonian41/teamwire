//! `kill -9` of `teamwire launch` around the moment the agent starts, then
//! `teamwire close` (`docs/test-strategy.md` section 3, `docs/failure-modes.md`
//! section 2: "Crash during launch, after the agent started").
#![expect(
    clippy::disallowed_methods,
    reason = "the test measures real elapsed time to place its kill points"
)]
#![expect(
    clippy::print_stderr,
    reason = "the kill window and the counts are printed so a failing run can be read"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::time::{Duration, Instant};

use rig::{Identity, Rig, assert_integrity, stop_agents, team_name, wait_idle};
use teamwire_core::{MemberState, Role};
use teamwire_store::Store;

const AGENT: &str = "tw_alpha_scout";

fn spec(work: &std::path::Path) -> String {
    serde_json::json!({"team": "alpha", "title": "Alpha", "children": [
        {"role": "scout", "title": "Scout", "mode": "pane", "cwd": work, "task": "look around"}]})
    .to_string()
}

/// The names of the agents the fake herdr holds.
fn agents(rig: &Rig) -> Vec<String> {
    let out = rig.herdr.run(&["agent", "list"]).unwrap();
    out.result()
        .and_then(|r| r.get("agents")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|a| a.get("name")?.as_str().map(str::to_owned))
        .collect()
}

fn scout(store: &Store) -> Option<teamwire_core::Member> {
    store
        .read(|tx| tx.member(&Role::parse("scout").unwrap()))
        .unwrap()
}

#[test]
fn kill9_after_the_agent_started_and_was_recorded_is_closed_by_close() {
    let mut rig = Rig::bare();
    let work = tempfile::tempdir().unwrap();
    // The first `agent prompt` is the task, typed after the place is recorded.
    let signal = rig.pause_after("agent prompt");
    let mut running = rig.start_with_stdin(&["launch"], &Identity::Nobody, &spec(work.path()));
    Rig::wait_for(&signal, &mut running);
    running.kill9();
    running.finish();
    rig.path_prefix = None;

    let store = rig.store();
    assert_integrity(&store);
    let member = scout(&store).unwrap();
    assert!(
        matches!(member.state, MemberState::Creating | MemberState::Started),
        "{:?}",
        member.state
    );
    assert!(
        member.place.is_some(),
        "the place was recorded before the kill"
    );
    assert_eq!(agents(&rig), [AGENT]);

    let out = rig.run(&["close", "alpha"], &Identity::Nobody);
    assert_eq!(out.code, Some(0), "{out:?}");
    assert!(
        agents(&rig).is_empty(),
        "the agent was left running: {out:?}"
    );
    assert!(
        !rig.home.path().join("teams/alpha").exists(),
        "the state directory was not deleted"
    );
    stop_agents(&rig);
}

#[test]
fn kill9_inside_the_agent_start_call_is_refused_by_close_until_the_agent_stops() {
    let mut rig = Rig::bare();
    let work = tempfile::tempdir().unwrap();
    let signal = rig.pause_after("agent start");
    let mut running = rig.start_with_stdin(&["launch"], &Identity::Nobody, &spec(work.path()));
    Rig::wait_for(&signal, &mut running);
    running.kill9();
    running.finish();
    rig.path_prefix = None;

    // The host started the agent; teamwire died before it heard the place.
    // The store is whole and names the member as half-made.
    let store = rig.store();
    assert_integrity(&store);
    let member = scout(&store).unwrap();
    assert_eq!(member.state, MemberState::Creating);
    assert!(member.place.is_none());
    assert_eq!(agents(&rig), [AGENT]);
    drop(store);

    // Close has no place to close, so it refuses by name and keeps the state.
    let refused = rig.run(&["close", "alpha"], &Identity::Nobody);
    assert_eq!(refused.code, Some(1), "{refused:?}");
    assert!(refused.stderr.contains(AGENT), "{refused:?}");
    assert!(
        rig.home.path().join("teams/alpha").exists(),
        "the state directory was deleted while an agent runs"
    );

    // Once the agent is stopped, close finishes and nothing is left running.
    stop_agents(&rig);
    let out = rig.run(&["close", "alpha"], &Identity::Nobody);
    assert_eq!(out.code, Some(0), "{out:?}");
    assert!(!rig.home.path().join("teams/alpha").exists());
    assert!(agents(&rig).is_empty());
}

#[test]
fn kill9_of_launch_at_random_points_leaves_a_store_close_can_finish() {
    let mut rng = rig::Rng::from_env("kill9_of_launch_at_random_points");
    let work = tempfile::tempdir().unwrap();
    // How long a whole launch takes here.
    let probe = Rig::bare();
    let started = Instant::now();
    let out = probe
        .start_with_stdin(&["launch"], &Identity::Nobody, &spec(work.path()))
        .finish();
    assert_eq!(out.code, Some(0), "{out:?}");
    // A launch sometimes takes far longer under load; kills past the usual
    // length add nothing, so the window is capped.
    let window = (started.elapsed() * 12 / 10).min(Duration::from_secs(3));
    stop_agents(&probe);
    let seed = rng.seed();
    eprintln!("kill window {window:?}");

    for i in 0..5 {
        let rig = Rig::bare();
        let delay: Duration = rng.up_to(window);
        let mut running = rig.start_with_stdin(&["launch"], &Identity::Nobody, &spec(work.path()));
        let begun = Instant::now();
        while begun.elapsed() < delay {
            std::hint::spin_loop();
        }
        running.kill9();
        running.finish();

        if !rig.home.path().join("teams/alpha").join("team.db").exists() {
            continue;
        }
        let Ok(store) = Store::open_existing(rig.home.path(), &team_name()) else {
            continue;
        };
        assert_integrity(&store);
        drop(store);
        wait_idle(&rig);
        // A kill inside the start call leaves a live agent that close cannot
        // find: it refuses by name, the agent is stopped, and close runs again.
        let mut out = rig.run(&["close", "alpha"], &Identity::Nobody);
        let mut refused = false;
        if out.code != Some(0) {
            refused = true;
            assert!(
                out.stderr.contains(AGENT),
                "seed {seed}, run {i}, killed at {delay:?}: close refused for another reason: {out:?}"
            );
            stop_agents(&rig);
            out = rig.run(&["close", "alpha"], &Identity::Nobody);
        }
        assert_eq!(
            out.code,
            Some(0),
            "seed {seed}, run {i}, killed at {delay:?}, refused first {refused}: {out:?}"
        );
        assert!(
            !rig.home.path().join("teams/alpha").exists(),
            "seed {seed}, run {i}, killed at {delay:?}: the state directory is still there"
        );
        stop_agents(&rig);
    }
}
