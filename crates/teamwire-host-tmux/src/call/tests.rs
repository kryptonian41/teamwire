use super::*;
use teamwire_core::RunOutput;
use teamwire_fakes::FakePlatform;

fn run(code: i32, stdout: &str, stderr: &str) -> Result<String, Failure> {
    let platform = FakePlatform::new();
    platform.queue_run(Ok(RunOutput {
        exit_code: code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
    }));
    call(
        &platform,
        Some("s"),
        &["list-panes"],
        None,
        Duration::from_secs(1),
    )
}

#[test]
fn stdout_is_returned_and_the_socket_is_named_first() {
    let platform = FakePlatform::new();
    platform.queue_run(Ok(RunOutput {
        exit_code: 0,
        stdout: b"%1\n".to_vec(),
        stderr: Vec::new(),
        timed_out: false,
    }));
    let got = call(
        &platform,
        Some("sock"),
        &["list-panes", "-a"],
        None,
        Duration::from_secs(2),
    );
    assert_eq!(got.unwrap(), "%1\n");
    let runs = platform.runs();
    let (sent, waited) = runs.first().unwrap();
    assert_eq!(sent.program, "tmux");
    assert_eq!(sent.args, ["-L", "sock", "list-panes", "-a"]);
    assert_eq!(*waited, Duration::from_secs(2));
}

#[test]
fn no_socket_means_no_dash_l() {
    let platform = FakePlatform::new();
    platform.queue_run(Ok(RunOutput {
        exit_code: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: false,
    }));
    call(
        &platform,
        None,
        &["list-panes"],
        None,
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(platform.runs().first().unwrap().0.args, ["list-panes"]);
}

#[test]
fn a_missing_server_and_a_missing_pane_are_told_apart() {
    let no_server = run(1, "", "no server running on /tmp/tmux-1/x\n").unwrap_err();
    assert!(matches!(no_server, Failure::NoServer(_)));
    let no_socket = run(
        1,
        "",
        "error connecting to /tmp/tmux-1/x (No such file or directory)",
    );
    assert!(matches!(no_socket.unwrap_err(), Failure::NoServer(_)));
    let no_pane = run(1, "", "can't find pane: %99\n").unwrap_err();
    assert!(matches!(no_pane, Failure::NoTarget(_)));
    assert_eq!(HostError::from(no_pane).code, HostErrorCode::NotFound);
}

#[test]
fn any_other_failure_is_other_with_the_text() {
    let e = HostError::from(run(1, "", "duplicate session: teamwire").unwrap_err());
    assert_eq!(e.code, HostErrorCode::Other);
    assert!(e.detail.contains("duplicate session"));
}

#[test]
fn a_timeout_is_timeout() {
    let platform = FakePlatform::new();
    platform.queue_run(Ok(RunOutput {
        exit_code: RunOutput::TIMEOUT_EXIT,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: true,
    }));
    let e =
        HostError::from(call(&platform, None, &["x"], None, Duration::from_secs(1)).unwrap_err());
    assert_eq!(e.code, HostErrorCode::Timeout);
}
