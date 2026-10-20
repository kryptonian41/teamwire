//! The in-process fakes pass their own contract suites, and each suite
//! fails for a fake that breaks one of its rules.

#![expect(
    clippy::unwrap_used,
    reason = "the helpers here build fixtures from literals; a bad literal should fail the test"
)]

use std::time::Duration;

use teamwire_core::{
    Address, HarnessCaps, HostCaps, HostError, HostErrorCode, Liveness, MemberKind, MemberLaunch,
    OtelConfig, Pid, PlaceState, PlatformError, PlatformErrorCode, Role, SessionId, SpawnRequest,
    TeamName,
};
use teamwire_fakes::contract::{
    EnvProbe, HarnessSetup, HostSetup, PlatformSetup, RunCase, harness_contract, host_contract,
    platform_contract,
};
use teamwire_fakes::{
    ESCALATED_TRUST_TEXT, FakeHarness, FakeHost, FakePlatform, Fault, PLAIN_TRUST_TEXT,
};

fn request(name: &Address) -> SpawnRequest {
    SpawnRequest {
        name: name.clone(),
        title: "contract".into(),
        cwd: "/work".into(),
        argv: vec!["cat".into()],
        env: Vec::new(),
    }
}

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn run_host_contract(host: &FakeHost) {
    let mut setup = HostSetup::new(host, &request);
    setup.echoes_input = true;
    host_contract(&setup);
}

#[test]
fn fake_host_passes_host_contract() {
    run_host_contract(&FakeHost::new());
}

#[test]
fn fake_host_without_typing_or_state_passes_host_contract() {
    let host = FakeHost::new();
    host.set_caps(HostCaps {
        can_type: false,
        reports_state: false,
        ..FakeHost::caps()
    });
    host_contract(&HostSetup::new(&host, &request));
}

#[test]
#[should_panic(expected = "contract violated: list must succeed")]
fn host_contract_fails_for_a_host_whose_list_breaks() {
    let host = FakeHost::new();
    host.inject_always(
        "list",
        Fault::Fail(HostError::new(HostErrorCode::Other, "down")),
    );
    run_host_contract(&host);
}

#[test]
#[should_panic(expected = "contract violated: state must succeed")]
fn host_contract_fails_for_a_host_whose_state_call_hangs() {
    let host = FakeHost::new();
    host.inject_always("state", Fault::Hang);
    run_host_contract(&host);
}

fn harness_setup(harness: &FakeHarness) -> HarnessSetup<'_> {
    let team = TeamName::parse("contract").unwrap();
    let role = Role::parse("api").unwrap();
    let live = sid("live-1");
    harness.add_session(
        live.clone(),
        vec![Pid::new(4242)],
        Liveness::Alive(PlaceState::Idle),
    );
    HarnessSetup {
        harness,
        member: MemberLaunch {
            address: Address::of(&team, &role),
            team,
            role,
            kind: MemberKind::Implementer,
            cwd: "/work".into(),
            session_id: sid("member-1"),
            model: None,
            effort: None,
            member_dir: "/home/teams/contract/members/api".into(),
            contract_path: "/home/contract.md".into(),
            identity: vec![
                ("TEAMWIRE_ROLE".into(), "api".into()),
                ("TEAMWIRE_TOKEN".into(), "s3cr3t-token-value".into()),
            ],
            otel: OtelConfig::default(),
            teamwire_bin: "/bin/teamwire".into(),
            plugins: std::collections::BTreeMap::new(),
            accept_trust: false,
        },
        turn_end_stdin: br#"{"session_id":"member-1","stop_hook_active":false}"#.to_vec(),
        turn_end_session: sid("member-1"),
        prompt_screen: "ready\n> ".into(),
        dialog_screen: PLAIN_TRUST_TEXT.into(),
        live_session: Some(live),
        unknown_session: sid("never-heard-of"),
        timeout: Duration::from_secs(5),
    }
}

#[test]
fn fake_harness_passes_harness_contract() {
    harness_contract(&harness_setup(&FakeHarness::new()));
}

#[test]
fn fake_harness_without_config_env_passes_harness_contract() {
    let harness = FakeHarness::new();
    harness.set_caps(HarnessCaps {
        session_env_via_config: false,
        choose_session_id: false,
        can_push: true,
        ..FakeHarness::caps()
    });
    harness_contract(&harness_setup(&harness));
}

#[test]
#[should_panic(expected = "the dialog screen is a dialog")]
fn harness_contract_fails_when_a_dialog_is_not_recognised() {
    let harness = FakeHarness::new();
    let mut setup = harness_setup(&harness);
    setup.dialog_screen = "an ordinary screen".into();
    harness_contract(&setup);
}

#[test]
#[should_panic(expected = "a running session is alive")]
fn harness_contract_fails_when_a_running_session_reads_dead() {
    let harness = FakeHarness::new();
    let setup = harness_setup(&harness);
    harness.set_liveness(&sid("live-1"), Liveness::Dead);
    harness_contract(&setup);
}

#[test]
#[should_panic(expected = "launch_spec keeps the value of TEAMWIRE_TOKEN out of argv")]
fn harness_contract_fails_when_a_secret_is_in_argv() {
    let harness = FakeHarness::new();
    let mut setup = harness_setup(&harness);
    // The fake puts the session id in argv, so a token with that value is there too.
    setup.member.identity = vec![("TEAMWIRE_TOKEN".into(), "member-1".into())];
    harness_contract(&setup);
}

#[test]
fn escalated_dialog_text_is_a_dialog_too() {
    let harness = FakeHarness::new();
    let mut setup = harness_setup(&harness);
    setup.dialog_screen = ESCALATED_TRUST_TEXT.into();
    harness_contract(&setup);
}

fn platform_setup<'a>(
    platform: &'a FakePlatform,
    dir: &'a std::path::Path,
    cases: Vec<RunCase>,
) -> PlatformSetup<'a> {
    let (live, dead, probe_pid) = (Pid::new(100), Pid::new(101), Pid::new(102));
    platform.set_alive(live, true);
    platform.set_alive(probe_pid, true);
    platform.set_env(probe_pid, "CONTRACT_PROBE", "probe-value");
    for case in &cases {
        platform.queue_run(Ok(case.expect.conforming_output()));
    }
    PlatformSetup {
        platform,
        dir,
        live_pid: live,
        dead_pid: Some(dead),
        env_probe: Some(EnvProbe {
            pid: probe_pid,
            var: "CONTRACT_PROBE".into(),
            value: "probe-value".into(),
        }),
        run_cases: cases,
        hold_lead: Pid::new(7),
    }
}

#[test]
fn fake_platform_passes_platform_contract() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    platform_contract(&platform_setup(
        &platform,
        dir.path(),
        RunCase::unix_standard(),
    ));
}

#[test]
#[should_panic(expected = "a timeout kills what the child started too: the grandchild is gone")]
fn platform_contract_fails_when_a_timeout_leaves_the_grandchild_running() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    let cases = RunCase::unix_standard();
    let setup = platform_setup(&platform, dir.path(), Vec::new());
    // The queue holds what a platform that kept the grandchild would say.
    platform.set_alive(Pid::new(555), true);
    for case in &cases {
        let mut out = case.expect.conforming_output();
        if case.expect.ends_grandchild {
            out.stdout = b"555\n".to_vec();
        }
        platform.queue_run(Ok(out));
    }
    platform_contract(&PlatformSetup {
        run_cases: cases,
        ..setup
    });
}

/// The run outputs for `cases`, where the grandchild case's first `killed`
/// runs time out before the child printed its pid.
fn queue_with_early_kills(platform: &FakePlatform, cases: &[RunCase], killed: usize) {
    for case in cases {
        if case.expect.ends_grandchild {
            for _ in 0..killed {
                platform.queue_run(Ok(teamwire_core::RunOutput {
                    stdout: Vec::new(),
                    ..case.expect.conforming_output()
                }));
            }
        }
        platform.queue_run(Ok(case.expect.conforming_output()));
    }
}

#[test]
fn platform_contract_tries_longer_timeouts_until_the_child_prints_its_pid() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    let cases = RunCase::unix_standard();
    let setup = platform_setup(&platform, dir.path(), Vec::new());
    queue_with_early_kills(&platform, &cases, 2);
    let grandchild = cases.iter().find(|c| c.expect.ends_grandchild).unwrap();
    let command = grandchild.command.clone();
    platform_contract(&PlatformSetup {
        run_cases: cases,
        ..setup
    });
    let timeouts: Vec<Duration> = platform
        .runs()
        .into_iter()
        .filter(|(cmd, _)| *cmd == command)
        .map(|(_, timeout)| timeout)
        .collect();
    assert_eq!(
        timeouts,
        [
            Duration::from_millis(300),
            Duration::from_secs(3),
            Duration::from_secs(30)
        ]
    );
}

#[test]
#[should_panic(
    expected = "a timeout kills what the child started too: the grandchild is gone: \"the child printed no pid\""
)]
fn platform_contract_fails_when_no_attempt_prints_a_pid() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    let cases = RunCase::unix_standard();
    let setup = platform_setup(&platform, dir.path(), Vec::new());
    queue_with_early_kills(&platform, &cases, 3);
    platform_contract(&PlatformSetup {
        run_cases: cases,
        ..setup
    });
}

#[test]
fn fake_platform_without_a_sleep_hold_passes_platform_contract() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::without_sleep_hold();
    platform_contract(&platform_setup(&platform, dir.path(), Vec::new()));
}

#[test]
#[should_panic(expected = "write_atomic must succeed")]
fn platform_contract_fails_on_a_full_disk() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    platform.fail_writes(Some(PlatformError::new(PlatformErrorCode::NoSpace, "full")));
    platform_contract(&platform_setup(&platform, dir.path(), Vec::new()));
}

#[test]
#[should_panic(expected = "free_bytes of a real directory")]
fn platform_contract_fails_when_free_space_reads_zero() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    platform.set_free_bytes(0);
    platform_contract(&platform_setup(&platform, dir.path(), Vec::new()));
}

#[test]
fn platform_contract_accepts_a_platform_that_reports_holds_as_unowned() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    platform.report_holds_as_unowned(true);
    platform_contract(&platform_setup(&platform, dir.path(), Vec::new()));
}

#[test]
#[should_panic(expected = "find_existing finds the hold just started")]
fn platform_contract_fails_when_find_existing_names_another_pid() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    platform.misreport_hold_pid(Some(Pid::new(1)));
    platform_contract(&platform_setup(&platform, dir.path(), Vec::new()));
}

#[test]
fn a_failing_platform_contract_still_releases_the_hold_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new();
    platform.misreport_hold_pid(Some(Pid::new(1)));
    let setup = platform_setup(&platform, dir.path(), Vec::new());
    let failed =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| platform_contract(&setup)));
    assert!(failed.is_err());
    let events = platform.hold_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            teamwire_fakes::HoldEvent::Released {
                outcome: teamwire_core::ReleaseOutcome::Released,
                ..
            }
        )),
        "{events:?}"
    );
}
