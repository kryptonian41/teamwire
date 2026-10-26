//! `answer_startup` against `FakeHost` screens: it waits for pi's input box
//! and refuses any dialog.

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Harness, HarnessErrorCode, HostError, HostErrorCode, Occupant, Pid, Place, PlaceState,
    StartupOutcome, TrustPolicy,
};
use teamwire_fakes::{FakeHost, FakePlatform, Fault};
use teamwire_harness_pi::{Pi, PiConfig, StartupPoll};

const PROMPT: &str = include_str!("screens/prompt.txt");
const TRUST: &str = include_str!("screens/trust.txt");

fn pi(steps: u32) -> Pi {
    Pi::new(
        Arc::new(FakePlatform::new()),
        PiConfig {
            agent_dir: "/nowhere/pi".into(),
            pi_bin: "pi".into(),
        },
    )
    .with_startup_poll(StartupPoll {
        step: Duration::from_millis(1),
        steps,
    })
}

fn place(host: &FakeHost, screen: &str) -> Place {
    let place = host.add_place(
        "tw_demo_api",
        PlaceState::Idle,
        Occupant {
            session: None,
            pids: vec![Pid::new(1)],
        },
    );
    host.set_screen(&place, screen);
    place
}

#[test]
fn the_input_box_ends_the_wait_with_no_dialog() {
    let host = FakeHost::new();
    let at = place(&host, PROMPT);
    let got = pi(10).answer_startup(&host, &at, TrustPolicy::Accept);
    assert_eq!(got.unwrap(), StartupOutcome::NoDialog);
    assert!(host.keys_sent(&at).is_empty());
}

#[test]
fn a_dialog_is_refused_and_never_answered() {
    for policy in [TrustPolicy::Accept, TrustPolicy::Refuse] {
        let host = FakeHost::new();
        let at = place(&host, TRUST);
        let err = pi(10).answer_startup(&host, &at, policy).unwrap_err();
        assert_eq!(err.code, HarnessErrorCode::Refused);
        assert!(err.detail.contains("--no-approve"), "{}", err.detail);
        assert!(host.keys_sent(&at).is_empty());
    }
}

#[test]
fn a_screen_that_never_shows_the_box_times_out() {
    let host = FakeHost::new();
    let at = place(&host, " ▀▀█  v1.4.0\n starting");
    let err = pi(5)
        .answer_startup(&host, &at, TrustPolicy::Accept)
        .unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Timeout);
}

#[test]
fn a_slow_read_is_read_again() {
    let host = FakeHost::new();
    let at = place(&host, PROMPT);
    host.inject(
        "read_tail",
        Fault::Fail(HostError::new(HostErrorCode::Busy, "busy")),
        2,
    );
    let got = pi(10).answer_startup(&host, &at, TrustPolicy::Accept);
    assert_eq!(got.unwrap(), StartupOutcome::NoDialog);
}

#[test]
fn another_host_failure_ends_the_wait() {
    let host = FakeHost::new();
    let at = place(&host, PROMPT);
    host.inject(
        "read_tail",
        Fault::Fail(HostError::new(HostErrorCode::NotFound, "gone")),
        1,
    );
    let err = pi(10)
        .answer_startup(&host, &at, TrustPolicy::Accept)
        .unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Host);
}
