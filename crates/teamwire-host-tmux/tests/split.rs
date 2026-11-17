//! `Tmux::split_run` and `Tmux::close_pane` against a real tmux server on a
//! private socket. Every test owns its server and kills it when it ends.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/server.rs"]
#[expect(dead_code, reason = "this file uses part of the shared rig")]
mod server;

use teamwire_core::{Host, HostErrorCode, Place, PlaceState, SplitRequest};

use server::{PrivateServer, SPAWN, T, request, screen_shows};

/// The `%<n>` part of a place id.
fn bare(place: &Place) -> &str {
    place.id.split('@').next().unwrap()
}

fn pane_format(server: &PrivateServer, place: &Place, format: &str) -> String {
    server
        .raw(&["display-message", "-p", "-t", bare(place), format])
        .trim()
        .to_owned()
}

#[test]
fn a_split_runs_beside_the_pane_with_its_title_folder_and_environment() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let old = tmux
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), SPAWN)
        .unwrap();
    let req = SplitRequest {
        label: "lead \u{b7} Demo team".to_owned(),
        cwd: dir.path().to_path_buf(),
        argv: vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "echo pane=$TMUX_PANE home=$TEAMWIRE_HOME dir=$(pwd -P); sleep 30".to_owned(),
        ],
        env: vec![("TEAMWIRE_HOME".to_owned(), "/state home".to_owned())],
    };
    let new = tmux.split_run(&old, &req, SPAWN).unwrap();
    assert_ne!(new, old);
    assert!(screen_shows(&tmux, &new, "home=/state home"));
    let shown = tmux.read_tail(&new, 20, T).unwrap();
    assert!(shown.contains(&format!("pane={}", bare(&new))), "{shown}");
    let real_dir = std::fs::canonicalize(dir.path()).unwrap();
    assert!(
        shown.contains(&format!("dir={}", real_dir.display())),
        "{shown}"
    );
    assert_eq!(
        pane_format(&server, &new, "#{pane_title}"),
        "lead \u{b7} Demo team"
    );
    assert_eq!(
        pane_format(&server, &new, "#{window_id}"),
        pane_format(&server, &old, "#{window_id}"),
        "the split is in the old pane's window"
    );
    assert_eq!(pane_format(&server, &new, "#{@teamwire_name}"), "");

    tmux.close_pane(&new, T).unwrap();
    assert_eq!(tmux.state(&new, T).unwrap(), PlaceState::Gone);
    assert_ne!(tmux.state(&old, T).unwrap(), PlaceState::Gone);
    tmux.close_pane(&new, T).unwrap();
}

#[test]
fn a_split_with_a_bad_variable_name_starts_nothing() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let old = tmux
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), SPAWN)
        .unwrap();
    let req = SplitRequest {
        label: "lead".to_owned(),
        cwd: dir.path().to_path_buf(),
        argv: vec!["cat".to_owned()],
        env: vec![("BAD NAME".to_owned(), "x".to_owned())],
    };
    let err = tmux.split_run(&old, &req, SPAWN).unwrap_err();
    assert_eq!(err.code, HostErrorCode::BadArgument);
    let panes = server.raw(&["list-panes", "-a", "-F", "#{pane_id}"]);
    assert_eq!(panes.lines().count(), 1, "{panes}");
}
