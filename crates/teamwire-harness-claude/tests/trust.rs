//! The folder-trust step: the dialog answers,
//! against `FakeHost` screens and against a real `fake-claude` process.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Harness, HarnessErrorCode, Host, HostCaps, HostError, HostId, Key, NoHost, Occupant, OneLine,
    Pid, Place, PlaceState, SpawnRequest, StartupOutcome, TrustPolicy,
};
use teamwire_fakes::{
    ClaudeScript, ESCALATED_TRUST_TEXT, FakeHerdrEnv, FakeHost, FakePlatform, PLAIN_TRUST_TEXT,
    Startup, TRUST_ESCALATION_MARKERS, TRUST_OPTION_TEXT,
};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig, StartupPoll};

fn fast() -> StartupPoll {
    StartupPoll {
        step: Duration::from_millis(1),
        steps: 3,
    }
}

fn claude_in(dir: &Path) -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: dir.join(".claude"),
            claude_json: dir.join(".claude.json"),
            claude_bin: "claude".into(),
        },
    )
    .with_startup_poll(fast())
}

fn host_with_screen(screen: &str) -> (FakeHost, Place) {
    let host = FakeHost::new();
    let place = host.add_place(
        "tw_demo_api",
        PlaceState::Blocked,
        Occupant {
            session: None,
            pids: vec![Pid::new(1)],
        },
    );
    host.set_screen(&place, screen);
    (host, place)
}

const PLAIN_DIALOG: &str = "Accessing workspace:\n /work\n\n Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your team).\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel";

fn answer(
    screen: &str,
    policy: TrustPolicy,
) -> (
    Result<StartupOutcome, teamwire_core::HarnessError>,
    Vec<Key>,
) {
    let dir = tempfile::tempdir().unwrap();
    let (host, place) = host_with_screen(screen);
    let got = claude_in(dir.path()).answer_startup(&host, &place, policy);
    (got, host.keys_sent(&place))
}

#[test]
fn escalated_trust_refused() {
    let (got, keys) = answer(
        &format!("{PLAIN_TRUST_TEXT}\n{TRUST_OPTION_TEXT}\n{ESCALATED_TRUST_TEXT}"),
        TrustPolicy::Accept,
    );
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty(), "nothing is typed into an escalated dialog");
}

#[test]
fn an_escalation_marker_far_above_the_last_forty_lines_still_refuses() {
    let padding = "\n ".repeat(60);
    let screen = format!("{PLAIN_DIALOG}\n{ESCALATED_TRUST_TEXT}{padding}");
    let (got, keys) = answer(&screen, TrustPolicy::Accept);
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty(), "nothing is typed into an escalated dialog");
}

#[test]
fn each_escalation_marker_alone_refuses() {
    for marker in TRUST_ESCALATION_MARKERS {
        let screen = format!("{PLAIN_DIALOG}\nThis folder: {marker} something");
        let (got, keys) = answer(&screen, TrustPolicy::Accept);
        assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated, "{marker}");
        assert!(keys.is_empty(), "{marker}");
    }
}

#[test]
fn an_escalation_marker_wrapped_across_lines_still_refuses() {
    let screen = format!("{PLAIN_DIALOG}\nThese will apply\nwithout asking.");
    let (got, keys) = answer(&screen, TrustPolicy::Accept);
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty());
}

#[test]
fn escalated_dialog_is_refused_even_when_the_policy_refuses() {
    let (got, keys) = answer(ESCALATED_TRUST_TEXT, TrustPolicy::Refuse);
    assert_eq!(got.unwrap(), StartupOutcome::RefusedEscalated);
    assert!(keys.is_empty());
}

#[test]
fn the_refuse_policy_answers_nothing_on_a_plain_dialog() {
    let (got, keys) = answer(PLAIN_DIALOG, TrustPolicy::Refuse);
    assert_eq!(got.unwrap_err().code, HarnessErrorCode::Refused);
    assert!(keys.is_empty());
}

#[test]
fn each_plain_marker_alone_types_nothing_because_no_selection_is_shown() {
    for marker in [TRUST_OPTION_TEXT, "Is this a project you created"] {
        let (got, keys) = answer(marker, TrustPolicy::Accept);
        // Without a visible pointer there is nothing to confirm, so no key
        // is sent: an Enter could pick "No, exit".
        assert_eq!(got.unwrap_err().code, HarnessErrorCode::Timeout, "{marker}");
        assert!(keys.is_empty(), "{marker}");
    }
}

// ---- a dialog that answers the keys, with a slow start ------------------------

/// A host whose screen is the trust dialog. `drop_downs` Down presses do
/// nothing, as when the dialog is not taking keys yet, and the screen shows
/// each change `lag` reads late. `chosen` is what Enter picked.
struct SlowDialog {
    host: FakeHost,
    place: Place,
    state: std::sync::Mutex<Slow>,
}

struct Slow {
    drop_downs: u32,
    lag: u32,
    cursor: usize,
    shown: usize,
    reads_since_change: u32,
    chosen: Option<usize>,
}

impl SlowDialog {
    fn new(drop_downs: u32, lag: u32) -> Self {
        let (host, place) = host_with_screen("");
        let dialog = Self {
            host,
            place,
            state: std::sync::Mutex::new(Slow {
                drop_downs,
                lag,
                cursor: 0,
                shown: 0,
                reads_since_change: 0,
                chosen: None,
            }),
        };
        dialog.render(0);
        dialog
    }

    fn render(&self, cursor: usize) {
        let mark = |i: usize| if i == cursor { "❯" } else { " " };
        self.host.set_screen(
            &self.place,
            &format!(
                "Quick safety check: Is this a project you created or one you trust?\n {} No, exit\n {} {TRUST_OPTION_TEXT}\n Enter to confirm · Esc to cancel",
                mark(0),
                mark(1)
            ),
        );
    }
}

impl Host for SlowDialog {
    fn id(&self) -> HostId {
        self.host.id()
    }
    fn capabilities(&self) -> HostCaps {
        self.host.capabilities()
    }
    fn spawn(&self, r: &SpawnRequest, t: Duration) -> Result<Place, HostError> {
        self.host.spawn(r, t)
    }
    fn submit(&self, p: &Place, x: &OneLine, t: Duration) -> Result<(), HostError> {
        self.host.submit(p, x, t)
    }
    fn state(&self, p: &Place, t: Duration) -> Result<PlaceState, HostError> {
        self.host.state(p, t)
    }
    fn occupant(&self, p: &Place, t: Duration) -> Result<Occupant, HostError> {
        self.host.occupant(p, t)
    }
    fn read_tail(&self, p: &Place, lines: u16, t: Duration) -> Result<String, HostError> {
        let mut st = self.state.lock().unwrap();
        st.reads_since_change += 1;
        if st.shown != st.cursor && st.reads_since_change > st.lag {
            st.shown = st.cursor;
            let shown = st.shown;
            drop(st);
            self.render(shown);
        }
        self.host.read_tail(p, lines, t)
    }
    fn send_keys(&self, p: &Place, keys: &[Key], t: Duration) -> Result<(), HostError> {
        self.host.send_keys(p, keys, t)?;
        let mut st = self.state.lock().unwrap();
        for key in keys {
            match key {
                Key::Down if st.drop_downs > 0 => st.drop_downs -= 1,
                Key::Down => {
                    st.cursor = 1;
                    st.reads_since_change = 0;
                }
                Key::Enter => {
                    st.chosen = Some(st.cursor);
                    if st.cursor == 1 {
                        drop(st);
                        self.host
                            .set_screen(&self.place, "welcome\n❯ \n  ? for shortcuts");
                        st = self.state.lock().unwrap();
                    }
                }
                Key::Char(_)
                | Key::Up
                | Key::Left
                | Key::Right
                | Key::Tab
                | Key::Escape
                | Key::Backspace
                | Key::CtrlC => {}
            }
        }
        Ok(())
    }
    fn close(&self, p: &Place, t: Duration) -> Result<(), HostError> {
        self.host.close(p, t)
    }
    fn list(&self, t: Duration) -> Result<Vec<Place>, HostError> {
        self.host.list(t)
    }
    fn live_agent_names(&self, t: Duration) -> Result<Vec<String>, HostError> {
        self.host.live_agent_names(t)
    }
    fn place_env_var(&self) -> Option<&'static str> {
        self.host.place_env_var()
    }
}

fn slow_answer(
    drop_downs: u32,
    lag: u32,
) -> (
    Result<StartupOutcome, teamwire_core::HarnessError>,
    Vec<Key>,
    Option<usize>,
) {
    let dir = tempfile::tempdir().unwrap();
    let dialog = SlowDialog::new(drop_downs, lag);
    let got = claude_in(dir.path())
        .with_startup_poll(StartupPoll {
            step: Duration::from_millis(1),
            steps: 12,
        })
        .answer_startup(&dialog, &dialog.place.clone(), TrustPolicy::Accept);
    let keys = dialog.host.keys_sent(&dialog.place);
    let chosen = dialog.state.lock().unwrap().chosen;
    (got, keys, chosen)
}

#[test]
fn the_answer_is_down_then_enter_and_never_a_digit() {
    let (got, keys, chosen) = slow_answer(0, 0);
    assert_eq!(got.unwrap(), StartupOutcome::AnsweredPlainTrust);
    assert_eq!(keys, [Key::Down, Key::Enter]);
    assert_eq!(chosen, Some(1));
}

#[test]
fn a_late_redraw_does_not_get_an_enter_on_no_exit() {
    let (got, keys, chosen) = slow_answer(0, 3);
    assert_eq!(got.unwrap(), StartupOutcome::AnsweredPlainTrust);
    assert_eq!(keys, [Key::Down, Key::Enter]);
    assert_eq!(chosen, Some(1));
}

#[test]
fn a_dropped_down_is_sent_again_before_any_enter() {
    let (got, keys, chosen) = slow_answer(1, 0);
    assert_eq!(got.unwrap(), StartupOutcome::AnsweredPlainTrust);
    assert_eq!(keys, [Key::Down, Key::Down, Key::Enter]);
    assert_eq!(chosen, Some(1));
}

#[test]
fn a_dialog_that_never_moves_gets_no_enter_and_times_out() {
    let (got, keys, chosen) = slow_answer(99, 0);
    assert_eq!(got.unwrap_err().code, HarnessErrorCode::Timeout);
    assert!(!keys.contains(&Key::Enter));
    assert_eq!(chosen, None);
}

#[test]
fn an_input_prompt_means_no_dialog() {
    let (got, keys) = answer(
        "welcome\n╭───╮\n│ ❯ │\n╰───╯\n  ? for shortcuts",
        TrustPolicy::Accept,
    );
    assert_eq!(got.unwrap(), StartupOutcome::NoDialog);
    assert!(keys.is_empty());
}

#[test]
fn a_screen_with_neither_prompt_nor_dialog_times_out_and_types_nothing() {
    let (got, keys) = answer("starting up...", TrustPolicy::Accept);
    assert_eq!(got.unwrap_err().code, HarnessErrorCode::Timeout);
    assert!(keys.is_empty());
}

#[test]
fn a_host_failure_is_reported_as_a_host_error() {
    let dir = tempfile::tempdir().unwrap();
    let host = FakeHost::new();
    let place = Place {
        host: HostId::from_static("fake"),
        id: "nope".into(),
    };
    let err = claude_in(dir.path())
        .answer_startup(&host, &place, TrustPolicy::Accept)
        .unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Host);
}

// ---- a real fake-claude process ---------------------------------------------

/// A host that drives one agent through `fake-herdr`'s command line.
struct HerdrCli<'a> {
    env: &'a FakeHerdrEnv,
}

impl HerdrCli<'_> {
    fn unsupported() -> HostError {
        NoHost
            .close(
                &Place {
                    host: HostId::from_static("none"),
                    id: String::new(),
                },
                Duration::ZERO,
            )
            .unwrap_err()
    }
}

impl Host for HerdrCli<'_> {
    fn id(&self) -> HostId {
        HostId::from_static("herdr")
    }
    fn capabilities(&self) -> HostCaps {
        NoHost.capabilities()
    }
    fn spawn(&self, _r: &SpawnRequest, _t: Duration) -> Result<Place, HostError> {
        Err(Self::unsupported())
    }
    fn submit(&self, _p: &Place, _x: &OneLine, _t: Duration) -> Result<(), HostError> {
        Err(Self::unsupported())
    }
    fn state(&self, _p: &Place, _t: Duration) -> Result<PlaceState, HostError> {
        Err(Self::unsupported())
    }
    fn occupant(&self, _p: &Place, _t: Duration) -> Result<Occupant, HostError> {
        Err(Self::unsupported())
    }
    fn read_tail(&self, _p: &Place, lines: u16, _t: Duration) -> Result<String, HostError> {
        let out = self
            .env
            .run(&[
                "agent",
                "read",
                "tw_demo_api",
                "--lines",
                &lines.to_string(),
            ])
            .map_err(|_| Self::unsupported())?;
        Ok(out.stdout)
    }
    fn send_keys(&self, _p: &Place, keys: &[Key], _t: Duration) -> Result<(), HostError> {
        for key in keys {
            let name = match key {
                Key::Down => "down",
                Key::Enter => "enter",
                Key::Char(_)
                | Key::Up
                | Key::Left
                | Key::Right
                | Key::Tab
                | Key::Escape
                | Key::Backspace
                | Key::CtrlC => "esc",
            };
            self.env
                .run(&["agent", "send-keys", "tw_demo_api", name])
                .map_err(|_| Self::unsupported())?;
        }
        Ok(())
    }
    fn close(&self, _p: &Place, _t: Duration) -> Result<(), HostError> {
        Err(Self::unsupported())
    }
    fn list(&self, _t: Duration) -> Result<Vec<Place>, HostError> {
        Ok(Vec::new())
    }
    fn live_agent_names(&self, _t: Duration) -> Result<Vec<String>, HostError> {
        Ok(Vec::new())
    }
    fn place_env_var(&self) -> Option<&'static str> {
        None
    }
}

fn started(startup: Startup) -> FakeHerdrEnv {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        startup,
        ..ClaudeScript::default()
    })
    .unwrap();
    let pane = env
        .run(&["workspace", "create", "--cwd", "/tmp", "--no-focus"])
        .unwrap()
        .result()
        .unwrap()
        .pointer("/root_pane/pane_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    // With a dialog open the start reports not-ready and leaves the agent running.
    env.run(&[
        "agent",
        "start",
        "tw_demo_api",
        "--kind",
        "claude",
        "--pane",
        &pane,
        "--timeout",
        "5000",
        "--",
        "-n",
        "tw_demo_api",
        "--session-id",
        "s-trust",
    ])
    .unwrap();
    env
}

fn real_answer(
    env: &FakeHerdrEnv,
    policy: TrustPolicy,
) -> Result<StartupOutcome, teamwire_core::HarnessError> {
    let dir = tempfile::tempdir().unwrap();
    let claude = claude_in(dir.path()).with_startup_poll(StartupPoll {
        step: Duration::from_millis(100),
        steps: 50,
    });
    let place = Place {
        host: HostId::from_static("herdr"),
        id: "p".into(),
    };
    claude.answer_startup(&HerdrCli { env }, &place, policy)
}

fn agent_status(env: &FakeHerdrEnv) -> String {
    env.run(&["agent", "get", "tw_demo_api"])
        .unwrap()
        .result()
        .unwrap()
        .pointer("/agent/agent_status")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn fake_claude_plain_dialog_is_answered_and_the_prompt_appears() {
    let env = started(Startup::Trust);
    assert_eq!(agent_status(&env), "blocked");
    assert_eq!(
        real_answer(&env, TrustPolicy::Accept).unwrap(),
        StartupOutcome::AnsweredPlainTrust
    );
    assert_ne!(agent_status(&env), "blocked");
}

#[test]
fn fake_claude_escalated_dialog_is_left_alone() {
    let env = started(Startup::EscalatedTrust);
    assert_eq!(
        real_answer(&env, TrustPolicy::Accept).unwrap(),
        StartupOutcome::RefusedEscalated
    );
    assert_eq!(agent_status(&env), "blocked");
}

#[test]
fn fake_claude_without_a_dialog_is_no_dialog() {
    let env = started(Startup::None);
    assert_eq!(
        real_answer(&env, TrustPolicy::Accept).unwrap(),
        StartupOutcome::NoDialog
    );
}
