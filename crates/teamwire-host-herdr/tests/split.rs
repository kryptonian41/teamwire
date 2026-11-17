//! `Herdr::split_run` and `Herdr::close_pane`, over `fake-herdr`.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "helpers in an integration-test file unwrap and index; a failure there fails the test"
)]

#[path = "support/helpers.rs"]
mod helpers;

use std::path::PathBuf;
use std::time::Duration;

use helpers::{T, World};
use teamwire_core::{Host, HostErrorCode, Pid, Place, SessionId, SplitRequest};
use teamwire_fakes::HerdrFaults;

const SESSION: &str = "5b0c8f7e-1d2a-4c3b-9e8f-0a1b2c3d4e5f";

fn beside(world: &World) -> Place {
    world
        .herdr
        .spawn(&world.request("tw_demo_api", "API"), T)
        .expect("spawn")
}

/// A fresh lead's command: its first prompt alone is longer than the
/// terminal's 1,024-byte line limit, as a real one can be.
fn lead(world: &World) -> SplitRequest {
    SplitRequest {
        label: "lead \u{b7} Demo team".to_owned(),
        cwd: world.env.root().to_path_buf(),
        argv: vec![
            "claude".to_owned(),
            "--session-id".to_owned(),
            SESSION.to_owned(),
            "Take over the run. ".repeat(80),
        ],
        env: vec![("TEAMWIRE_HOME".to_owned(), "/state/home".to_owned())],
    }
}

fn calls(world: &World, command: &str) -> Vec<Vec<String>> {
    world
        .calls_of(command)
        .into_iter()
        .map(|c| c.argv)
        .collect()
}

/// The tab herdr reports for a pane.
fn tab_of(world: &World, pane: &str) -> String {
    let out = world.env.run(&["pane", "get", pane]).expect("pane get");
    out.result().expect("pane info")["pane"]["tab_id"]
        .as_str()
        .expect("tab id")
        .to_owned()
}

/// The folder that holds the run file the `pane run` line names.
fn run_folder(line: &str) -> PathBuf {
    let file = line.strip_prefix("sh ").expect("a sh line");
    PathBuf::from(file.trim_matches('\''))
        .parent()
        .expect("the run file's folder")
        .to_path_buf()
}

/// Waits for the session `fake-claude` writes once it runs.
fn session_in(world: &World, place: &Place) -> Option<SessionId> {
    for _ in 0..200 {
        let found = world.herdr.occupant(place, T).ok().and_then(|o| o.session);
        if found.is_some() {
            return found;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

#[test]
fn a_split_runs_the_command_beside_the_pane_with_its_label_folder_and_environment() {
    let world = World::new();
    let old = beside(&world);
    let req = lead(&world);
    let new = world.herdr.split_run(&old, &req, T).expect("split_run");

    let cwd = req.cwd.display().to_string();
    assert_eq!(
        calls(&world, "pane split"),
        [[
            "pane",
            "split",
            old.id.as_str(),
            "--direction",
            "right",
            "--cwd",
            cwd.as_str(),
            "--no-focus",
            "--env",
            "TEAMWIRE_HOME=/state/home",
        ]]
    );
    assert_eq!(
        calls(&world, "pane rename"),
        [["pane", "rename", new.id.as_str(), "lead \u{b7} Demo team"]]
    );
    let runs = calls(&world, "pane run");
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0][2], new.id);
    assert!(runs[0][3].len() < 1024, "{}", runs[0][3]);
    assert_eq!(tab_of(&world, &new.id), tab_of(&world, &old.id));

    // The new pane runs the command: herdr sees its session, and its process
    // got the split's variable and herdr's pane variable.
    let session = session_in(&world, &new).expect("the new session runs");
    assert_eq!(session.as_str(), SESSION);
    let files = world.env.session_files();
    let pid = files
        .iter()
        .find_map(|f| {
            let doc: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
            (doc["sessionId"] == SESSION).then(|| doc["pid"].as_u64())?
        })
        .expect("the new session's file");
    let platform = teamwire_platform::native();
    let pid = Pid::new(u32::try_from(pid).expect("pid"));
    assert_eq!(
        platform.env_of(pid, "TEAMWIRE_HOME").expect("env"),
        Some("/state/home".to_owned())
    );
    assert_eq!(
        platform.env_of(pid, "HERDR_PANE_ID").expect("env"),
        Some(new.id.clone())
    );
    assert!(
        !run_folder(&runs[0][3]).exists(),
        "the run file removes its folder"
    );
    world.herdr.close_pane(&new, T).expect("close_pane");
}

#[test]
fn a_failed_rename_closes_the_new_pane_and_runs_nothing() {
    let faults = HerdrFaults::new().error("pane rename", None, Some(1), "internal_error");
    let world = World::with_faults(Some(&faults));
    let old = beside(&world);
    let err = world
        .herdr
        .split_run(&old, &lead(&world), T)
        .expect_err("the rename fails");
    assert_eq!(err.code, HostErrorCode::Other, "{err}");
    let made = world.env.calls();
    let closes = calls(&world, "pane close");
    assert_eq!(closes.len(), 1, "{made:?}");
    assert!(calls(&world, "pane run").is_empty(), "{made:?}");
    let new = &closes[0][2];
    assert_ne!(new, &old.id, "the old pane stays");
    assert_eq!(
        world
            .env
            .run(&["pane", "get", new])
            .expect("get")
            .error_code(),
        Some("pane_not_found".to_owned())
    );
}

#[test]
fn a_failed_run_closes_the_new_pane_and_removes_the_run_folder() {
    let faults = HerdrFaults::new().error("pane run", None, Some(1), "internal_error");
    let world = World::with_faults(Some(&faults));
    let old = beside(&world);
    let err = world
        .herdr
        .split_run(&old, &lead(&world), T)
        .expect_err("the run fails");
    assert_eq!(err.code, HostErrorCode::Other, "{err}");
    let runs = calls(&world, "pane run");
    assert_eq!(runs.len(), 1, "{runs:?}");
    let new = &runs[0][2];
    assert_eq!(
        calls(&world, "pane close"),
        [["pane", "close", new.as_str()]]
    );
    assert!(
        !run_folder(&runs[0][3]).exists(),
        "a failed split leaves no run folder"
    );
}
