//! `Tmux` against a real tmux server on a private socket. Every test owns
//! its server and kills it when it ends, on every path.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/server.rs"]
mod server;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use teamwire_core::{Address, Host, HostErrorCode, Key, Place, PlaceState, SpawnRequest, one_line};
use teamwire_fakes::contract::{HostSetup, host_contract};

use server::{PrivateServer, request, screen_shows};

const T: Duration = Duration::from_secs(10);

/// The `%<n>` part of a place id, which is what `TMUX_PANE` holds.
fn bare(place: &Place) -> &str {
    place.id.split('@').next().unwrap()
}

#[test]
fn passes_host_contract_on_a_private_socket() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let make = |name: &Address| SpawnRequest {
        name: name.clone(),
        title: "contract".to_owned(),
        cwd: dir.path().to_path_buf(),
        argv: vec!["cat".to_owned()],
        env: Vec::new(),
    };
    let mut setup = HostSetup::new(&tmux, &make);
    setup.echoes_input = true;
    setup.lists_agent_names = false;
    host_contract(&setup);
}

#[test]
fn the_environment_reaches_the_program_at_spawn() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(
        "tw_t_a",
        dir.path(),
        &[
            "sh",
            "-c",
            "echo pane=$TMUX_PANE token=$TEAMWIRE_TEST_TOKEN; sleep 30",
        ],
    );
    req.env = vec![("TEAMWIRE_TEST_TOKEN".to_owned(), "s3cret value".to_owned())];
    let place = tmux.spawn(&req, T).unwrap();
    assert!(screen_shows(&tmux, &place, "token=s3cret value"));
    let shown = tmux.read_tail(&place, 20, T).unwrap();
    assert!(
        shown.contains(&format!("pane={}", bare(&place))),
        "TMUX_PANE names the place: {shown}"
    );
}

#[test]
fn the_working_directory_is_the_requested_one() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    // macOS reaches a temporary directory as /var/... or /private/var/...
    let unique = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let req = request("tw_t_a", dir.path(), &["sh", "-c", "pwd; sleep 30"]);
    let place = tmux.spawn(&req, T).unwrap();
    assert!(screen_shows(&tmux, &place, &unique));
}

#[test]
fn list_and_names_hold_only_panes_this_adapter_started() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let a = tmux
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    // A pane somebody else made on the same server.
    server.raw(&["new-window", "-d", "--", "cat"]);
    assert!(server.raw(&["list-panes", "-a"]).lines().count() == 2);
    assert_eq!(tmux.list(T).unwrap(), [a]);
    assert_eq!(tmux.live_agent_names(T).unwrap(), ["tw_t_a"]);
}

#[test]
fn list_with_no_server_is_empty_and_a_place_is_gone() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    assert_eq!(tmux.list(T).unwrap(), []);
    assert_eq!(tmux.live_agent_names(T).unwrap(), Vec::<String>::new());
    let place = Place {
        host: tmux.id(),
        id: "%0".to_owned(),
    };
    assert_eq!(tmux.state(&place, T).unwrap(), PlaceState::Gone);
    assert_eq!(
        tmux.close(&place, T).unwrap_err().code,
        HostErrorCode::NotFound
    );
}

#[test]
fn a_session_or_window_name_is_never_a_place() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    tmux.spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    // `teamwire` is the session `spawn` made: tmux would resolve it to a pane.
    let by_name = Place {
        host: tmux.id(),
        id: "teamwire".to_owned(),
    };
    assert_eq!(
        tmux.occupant(&by_name, T).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(
        tmux.close(&by_name, T).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(tmux.list(T).unwrap().len(), 1, "nothing was closed");
}

#[test]
fn occupant_holds_the_panes_process_and_its_children() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let place = tmux
        .spawn(
            &request("tw_t_a", dir.path(), &["sh", "-c", "sleep 30 & wait"]),
            T,
        )
        .unwrap();
    let want = server.raw(&["display-message", "-p", "-t", bare(&place), "#{pane_pid}"]);
    let mut pids = Vec::new();
    for _ in 0..100 {
        let occupant = tmux.occupant(&place, T).unwrap();
        assert_eq!(occupant.session, None);
        pids = occupant.pids;
        if pids.len() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(pids.first().unwrap().to_string(), want.trim());
    assert!(
        pids.len() >= 2,
        "the shell's child is in the tree: {pids:?}"
    );
}

#[test]
fn text_with_shell_and_tmux_special_characters_arrives_whole() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let place = tmux
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    let text = r#"a; b "q" 'r' $HOME `x` \n #{pane_id} -t %1 Enter"#;
    tmux.submit(&place, &one_line(text, 200), T).unwrap();
    assert!(
        screen_shows(&tmux, &place, text),
        "{}",
        tmux.read_tail(&place, 20, T).unwrap()
    );
    // Enter was pressed: cat echoes the line back, so it shows twice. The
    // echo comes after the typed text, so wait for it as for the text.
    let mut shown = String::new();
    for _ in 0..100 {
        shown = tmux.read_tail(&place, 20, T).unwrap();
        if shown.matches("#{pane_id}").count() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(shown.matches("#{pane_id}").count() >= 2, "{shown}");
}

#[test]
fn a_paste_buffer_is_not_left_behind() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let place = tmux
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    tmux.submit(&place, &one_line("hello", 20), T).unwrap();
    assert_eq!(server.raw(&["list-buffers"]).trim(), "");
    let gone = Place {
        host: tmux.id(),
        id: "%99".to_owned(),
    };
    assert_eq!(
        tmux.submit(&gone, &one_line("hello", 20), T)
            .unwrap_err()
            .code,
        HostErrorCode::NotFound
    );
    assert_eq!(
        server.raw(&["list-buffers"]).trim(),
        "",
        "a failed paste drops its buffer too"
    );
}

#[test]
fn send_keys_types_characters_literally_and_presses_named_keys() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let place = tmux
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    let keys = [
        Key::Char('-'),
        Key::Char(';'),
        Key::Char('E'),
        Key::Char('n'),
        Key::Enter,
    ];
    tmux.send_keys(&place, &keys, T).unwrap();
    assert!(screen_shows(&tmux, &place, "-;En"));
    tmux.send_keys(&place, &[], T).unwrap();
}

#[test]
fn a_closed_pane_is_gone_and_the_others_stay() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let a = tmux
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    let b = tmux
        .spawn(&request("tw_t_b", dir.path(), &["cat"]), T)
        .unwrap();
    assert_ne!(a, b);
    tmux.close(&a, T).unwrap();
    assert_eq!(tmux.state(&a, T).unwrap(), PlaceState::Gone);
    assert_eq!(tmux.state(&b, T).unwrap(), PlaceState::Unknown);
    assert_eq!(tmux.list(T).unwrap(), [b]);
}

#[test]
fn spawn_needs_a_program_and_a_sane_environment_name() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let none = request("tw_t_a", dir.path(), &[]);
    assert_eq!(
        tmux.spawn(&none, T).unwrap_err().code,
        HostErrorCode::BadArgument
    );
    let mut bad = request("tw_t_a", dir.path(), &["cat"]);
    bad.env = vec![("A=B".to_owned(), "c".to_owned())];
    assert_eq!(
        tmux.spawn(&bad, T).unwrap_err().code,
        HostErrorCode::BadArgument
    );
    assert_eq!(tmux.list(T).unwrap(), []);
}

#[test]
fn the_capabilities_are_the_documented_ones() {
    let server = PrivateServer::new();
    let caps = server.tmux().capabilities();
    assert!(caps.can_type && caps.sets_env_at_spawn);
    assert!(!caps.reports_state && !caps.guards_dialogs && !caps.knows_session_of_place);
    assert_eq!(server.tmux().place_env_var(), Some("TMUX_PANE"));
}

#[test]
fn close_refuses_a_pane_without_the_teamwire_mark() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    // One marked pane keeps the server alive; the plain pane is "the user's".
    tmux.spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    let id = server
        .raw(&["new-window", "-d", "-P", "-F", "#{pane_id}", "cat"])
        .trim()
        .to_owned();
    let theirs = Place {
        host: tmux.id(),
        id: id.clone(),
    };
    let err = tmux.close(&theirs, T).unwrap_err();
    assert_eq!(err.code, HostErrorCode::NotFound);
    assert!(
        server
            .raw(&["list-panes", "-a", "-F", "#{pane_id}"])
            .contains(&id)
    );
}

#[test]
fn a_title_ending_in_a_semicolon_still_spawns() {
    let server = PrivateServer::new();
    let tmux = server.tmux();
    let dir = tempfile::tempdir().unwrap();
    let mut req = request("tw_t_a", dir.path(), &["cat"]);
    req.title = "x;".to_owned();
    tmux.spawn(&req, T).unwrap();
    let names = server.raw(&["list-windows", "-a", "-F", "#{window_name}"]);
    assert!(names.lines().any(|n| n == "x;"), "{names}");
}

/// Records the argv of every command, then runs it on the wrapped platform.
struct Recording {
    inner: std::sync::Arc<dyn teamwire_core::Platform>,
    argv: std::sync::Mutex<Vec<String>>,
    /// The mode of each file `write_atomic` wrote, read right after the write.
    modes: std::sync::Mutex<Vec<u32>>,
}

impl teamwire_core::Platform for Recording {
    fn pid_alive(&self, pid: teamwire_core::Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn env_of(
        &self,
        pid: teamwire_core::Pid,
        var: &str,
    ) -> Result<Option<String>, teamwire_core::PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(
        &self,
        cmd: &teamwire_core::Command,
        timeout: Duration,
    ) -> Result<teamwire_core::RunOutput, teamwire_core::PlatformError> {
        self.argv.lock().unwrap().extend(cmd.args.iter().cloned());
        self.inner.run(cmd, timeout)
    }
    fn write_atomic(
        &self,
        path: &std::path::Path,
        bytes: &[u8],
    ) -> Result<(), teamwire_core::PlatformError> {
        self.inner.write_atomic(path, bytes)?;
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
        self.modes.lock().unwrap().push(mode);
        Ok(())
    }
    fn sleep_hold(&self) -> Option<&dyn teamwire_core::SleepHold> {
        self.inner.sleep_hold()
    }
    fn free_bytes(&self, path: &std::path::Path) -> Result<u64, teamwire_core::PlatformError> {
        self.inner.free_bytes(path)
    }
    fn on_network_fs(&self, path: &std::path::Path) -> Result<bool, teamwire_core::PlatformError> {
        self.inner.on_network_fs(path)
    }
}

#[test]
fn the_environment_values_never_appear_in_any_tmux_argument() {
    let server = PrivateServer::new();
    let recording = std::sync::Arc::new(Recording {
        inner: std::sync::Arc::clone(&server.platform),
        argv: std::sync::Mutex::default(),
        modes: std::sync::Mutex::default(),
    });
    let tmux = teamwire_host_tmux::Tmux::new(recording.clone()).with_socket(server.socket.clone());
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(
        "tw_t_a",
        dir.path(),
        &["sh", "-c", "echo token=$TEAMWIRE_TEST_TOKEN; sleep 30"],
    );
    req.env = vec![("TEAMWIRE_TEST_TOKEN".to_owned(), "it's s3cret".to_owned())];
    let place = tmux.spawn(&req, T).unwrap();
    assert!(screen_shows(&tmux, &place, "token=it's s3cret"));
    let argv = recording.argv.lock().unwrap().join("\n");
    assert!(!argv.contains("s3cret"), "a tmux argument holds the value");
}

#[test]
fn the_environment_file_is_readable_by_its_owner_only() {
    let server = PrivateServer::new();
    let recording = std::sync::Arc::new(Recording {
        inner: std::sync::Arc::clone(&server.platform),
        argv: std::sync::Mutex::default(),
        modes: std::sync::Mutex::default(),
    });
    let tmux = teamwire_host_tmux::Tmux::new(recording.clone()).with_socket(server.socket.clone());
    let dir = tempfile::tempdir().unwrap();
    let mut req = request("tw_t_a", dir.path(), &["sleep", "30"]);
    req.env = vec![("TEAMWIRE_TEST_TOKEN".to_owned(), "s3cret".to_owned())];
    tmux.spawn(&req, T).unwrap();
    assert_eq!(*recording.modes.lock().unwrap(), [0o600]);
}

#[test]
fn a_place_records_its_server_and_any_adapter_can_reach_it() {
    let server = PrivateServer::new();
    let dir = tempfile::tempdir().unwrap();
    let place = server
        .tmux()
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    let (pane, path) = place
        .id
        .split_once('@')
        .expect("the place records the socket path");
    assert!(pane.starts_with('%'), "{}", place.id);
    assert!(path.ends_with(&server.socket), "{path}");

    // An adapter on another server, and one on none, as a shell outside tmux
    // builds it: the place names the server, so neither asks the wrong one.
    let other = PrivateServer::new();
    for tmux in [
        other.tmux(),
        teamwire_host_tmux::Tmux::new(std::sync::Arc::clone(&server.platform)),
    ] {
        assert_eq!(tmux.state(&place, T).unwrap(), PlaceState::Unknown);
        tmux.submit(&place, &one_line("from afar", 20), T).unwrap();
        assert!(screen_shows(&tmux, &place, "from afar"));
        assert!(tmux.occupant(&place, T).unwrap().pids.len() == 1);
    }
    teamwire_host_tmux::Tmux::new(std::sync::Arc::clone(&server.platform))
        .close(&place, T)
        .unwrap();
    assert_eq!(server.tmux().list(T).unwrap(), []);
}

#[test]
fn a_bare_pane_id_goes_to_the_server_the_client_value_names() {
    let server = PrivateServer::new();
    let dir = tempfile::tempdir().unwrap();
    let placed = server
        .tmux()
        .spawn(&request("tw_t_a", dir.path(), &["cat"]), T)
        .unwrap();
    let (pane, path) = placed.id.split_once('@').unwrap();
    // What an older team stored, and what a locate finds in `TMUX_PANE`.
    let bare = Place {
        host: placed.host.clone(),
        id: pane.to_owned(),
    };
    let tmux = teamwire_host_tmux::Tmux::new(std::sync::Arc::clone(&server.platform))
        .with_client(&format!("{path},4242,0"));
    assert_eq!(tmux.state(&bare, T).unwrap(), PlaceState::Unknown);
    assert_eq!(
        tmux.list(T).unwrap(),
        [placed],
        "list records the server too"
    );
}
