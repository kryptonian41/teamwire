//! Check of the tmux adapter against the installed tmux, run by hand:
//!
//! ```text
//! cargo run -p teamwire-host-tmux --example tmux_live_check --locked
//! ```
//!
//! It starts its own tmux server on a private socket (`-L teamwire-live-<pid>`),
//! so it never touches the server you work in, and it kills that server when
//! it ends. It spawns one pane with an identity variable, types a line into
//! it, submits it, reads the screen, asks for the state and closes the pane,
//! then prints PASS or FAIL for each step. No test runs it. The herdr
//! counterpart is `teamwire-host-herdr`'s `live_check`.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a hand-run example reports to the person who ran it"
)]

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{Address, Command, Host, Key, PlaceState, Platform, SpawnRequest, one_line};
use teamwire_fakes::NoTmuxConfig;
use teamwire_host_tmux::Tmux;

const T: Duration = Duration::from_secs(10);

/// Prints one line per step and remembers a failure.
struct Report {
    failed: bool,
}

impl Report {
    fn step(&mut self, what: &str, ok: bool, detail: &str) {
        println!("{} {what}{}", if ok { "PASS" } else { "FAIL" }, {
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        });
        self.failed |= !ok;
    }
}

/// Looks at the screen every 100 ms for up to 5 s until it holds `text`.
fn screen_holds(host: &dyn Host, place: &teamwire_core::Place, text: &str, times: usize) -> bool {
    for _ in 0..50 {
        match host.read_tail(place, 40, T) {
            Ok(tail) if tail.matches(text).count() >= times => return true,
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    false
}

/// The file tmux keeps for `-L <socket>`: `<TMUX_TMPDIR or /tmp>/tmux-<uid>/<socket>`.
fn socket_file(socket: &str) -> Option<std::path::PathBuf> {
    let out = teamwire_platform::native()
        .run(&Command::new("id").arg("-u"), T)
        .ok()?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let base =
        std::env::var_os("TMUX_TMPDIR").map_or_else(|| "/tmp".into(), std::path::PathBuf::from);
    Some(base.join(format!("tmux-{uid}")).join(socket))
}

fn main() -> ExitCode {
    let socket = format!("teamwire-live-{}", std::process::id());
    // Without a tmux config: the check should not wait on the user's plugins.
    let platform: Arc<dyn Platform> = Arc::new(NoTmuxConfig::new(teamwire_platform::native()));
    let tmux = Tmux::new(Arc::clone(&platform)).with_socket(socket.clone());
    let mut report = Report { failed: false };

    run(&tmux, &platform, &mut report);

    let kill = Command::new("tmux").args(["-L", &socket, "kill-server"]);
    drop(platform.run(&kill, T));
    // A killed server leaves its socket file behind.
    if let Some(file) = socket_file(&socket) {
        drop(std::fs::remove_file(file));
    }
    if report.failed {
        eprintln!("tmux live check FAILED");
        ExitCode::FAILURE
    } else {
        println!("tmux live check passed");
        ExitCode::SUCCESS
    }
}

fn run(tmux: &Tmux, platform: &Arc<dyn Platform>, report: &mut Report) {
    let Ok(name) = Address::parse("tw_live_check") else {
        report.step("address", false, "tw_live_check is not an address");
        return;
    };
    let req = SpawnRequest {
        name,
        title: "tmux live check".to_owned(),
        team_title: "Demo team".to_owned(),
        generation: 1,
        cwd: std::env::temp_dir(),
        // `cat` echoes each submitted line, so a submitted line shows twice.
        argv: vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "echo identity=$TEAMWIRE_LIVE; exec cat".to_owned(),
        ],
        env: vec![("TEAMWIRE_LIVE".to_owned(), "token one".to_owned())],
    };
    let place = match tmux.spawn(&req, T) {
        Ok(place) => place,
        Err(e) => {
            report.step("spawn", false, &e.to_string());
            return;
        }
    };
    report.step("spawn", true, &format!("place {}", place.id));
    report.step(
        "place records its socket",
        place.id.contains('@'),
        &place.id,
    );

    report.step(
        "identity environment reaches the program",
        screen_holds(tmux, &place, "identity=token one", 1),
        "",
    );

    // Typing a line without submitting it: the characters show once.
    let typed: Result<Vec<()>, _> = "typed"
        .chars()
        .map(|c| tmux.send_keys(&place, &[Key::Char(c)], T))
        .collect();
    report.step(
        "send_keys types characters",
        typed.is_ok() && screen_holds(tmux, &place, "typed", 1),
        "",
    );
    report.step(
        "send_keys presses Enter",
        tmux.send_keys(&place, &[Key::Enter], T).is_ok() && screen_holds(tmux, &place, "typed", 2),
        "the line is echoed back",
    );

    let submitted = tmux.submit(&place, &one_line("submitted line", 100), T);
    report.step(
        "submit pastes the line and presses Enter",
        submitted.is_ok() && screen_holds(tmux, &place, "submitted line", 2),
        &format!("{submitted:?}"),
    );

    let state = tmux.state(&place, T);
    report.step(
        "state of a live pane",
        state == Ok(PlaceState::Unknown),
        &format!("{state:?} (tmux reports no state; close asks the harness)"),
    );
    let occupant = tmux.occupant(&place, T);
    report.step(
        "occupant lists the pane's processes",
        occupant.as_ref().is_ok_and(|o| !o.pids.is_empty()),
        &format!("{occupant:?}"),
    );
    let listed = tmux.list(T);
    report.step(
        "list holds the place",
        listed.as_ref().is_ok_and(|l| l.contains(&place)),
        &format!("{listed:?}"),
    );

    // A second adapter that is told no server, as a shell outside tmux has.
    let outside = Tmux::new(Arc::clone(platform));
    let seen = outside.state(&place, T);
    report.step(
        "an adapter with no server still reaches the place",
        seen == Ok(PlaceState::Unknown),
        &format!("{seen:?}"),
    );

    let closed = tmux.close(&place, &req.name, T);
    let after = tmux.state(&place, T);
    report.step(
        "close removes the pane",
        closed.is_ok() && after == Ok(PlaceState::Gone),
        &format!("close {closed:?}, then state {after:?}"),
    );
}
