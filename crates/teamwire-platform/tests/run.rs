//! `Platform::run`: output, exit codes, stdin, cwd, environment, timeouts.

use std::time::Duration;

use teamwire_core::{Clock, Command, PlatformErrorCode};
use teamwire_platform::{SystemClock, native};

const LONG: Duration = Duration::from_secs(20);

/// The timeout for a test whose shell must reach a line before the timeout
/// kills it. On a busy machine a process start has taken over a second, and
/// a shorter timeout kills the shell before that line runs.
const TIMES_OUT: Duration = Duration::from_secs(3);

fn sh(script: &str) -> Command {
    Command::new("/bin/sh").arg("-c").arg(script)
}

fn alive(pid: u32) -> bool {
    native().pid_alive(teamwire_core::Pid::new(pid))
}

#[test]
fn captures_stdout_stderr_and_exit_code() {
    let out = native()
        .run(&sh("echo out; echo err >&2; exit 3"), LONG)
        .unwrap();
    assert_eq!(out.exit_code, 3);
    assert_eq!(out.stdout, b"out\n");
    assert_eq!(out.stderr, b"err\n");
    assert!(!out.timed_out);
    assert!(!out.success());
}

#[test]
fn huge_timeout_does_not_panic() {
    let out = native().run(&sh("true"), Duration::MAX).unwrap();
    assert_eq!(out.exit_code, 0);
    assert!(!out.timed_out);
}

#[test]
fn success_is_exit_zero() {
    assert!(native().run(&sh("true"), LONG).unwrap().success());
}

#[test]
fn stdin_bytes_reach_the_child_and_are_closed() {
    let out = native()
        .run(&Command::new("cat").stdin(b"hello".to_vec()), LONG)
        .unwrap();
    assert_eq!(out.stdout, b"hello");
    assert_eq!(out.exit_code, 0);
}

#[test]
fn no_stdin_gives_an_empty_stdin() {
    let out = native().run(&Command::new("cat"), LONG).unwrap();
    assert_eq!(out.stdout, b"");
    assert!(!out.timed_out);
}

#[test]
fn large_stdin_and_output_do_not_deadlock() {
    let bytes = vec![b'x'; 1 << 20];
    let out = native()
        .run(&Command::new("cat").stdin(bytes.clone()), LONG)
        .unwrap();
    assert_eq!(out.stdout, bytes);
}

#[test]
fn cwd_is_honoured() {
    let dir = tempfile::tempdir().unwrap();
    let want = dir.path().canonicalize().unwrap();
    let out = native()
        .run(&Command::new("pwd").cwd(dir.path()), LONG)
        .unwrap();
    let got = String::from_utf8(out.stdout).unwrap();
    assert_eq!(std::path::Path::new(got.trim()), want);
}

#[test]
fn output_read_before_the_pipes_close_is_kept() {
    // The shell exits at once, but the background sleep it leaves behind
    // holds stdout open past the wait for the readers. What the shell
    // printed is still returned.
    let out = native()
        .run(&sh("echo hello; echo oops >&2; sleep 4 &"), LONG)
        .unwrap();
    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stdout, b"hello\n");
    assert_eq!(out.stderr, b"oops\n");
}

#[test]
fn env_is_added_on_top_of_the_inherited_environment() {
    let out = native()
        .run(
            &sh("echo \"$TW_T03_VAR:${PATH:+has_path}\"").env("TW_T03_VAR", "v"),
            LONG,
        )
        .unwrap();
    assert_eq!(out.stdout, b"v:has_path\n");
}

#[test]
fn env_clear_drops_the_inherited_environment() {
    let out = native()
        .run(
            &sh("echo \"$TW_T03_VAR:${HOME:-no_home}\"")
                .env_clear()
                .env("TW_T03_VAR", "v"),
            LONG,
        )
        .unwrap();
    assert_eq!(out.stdout, b"v:no_home\n");
}

#[test]
fn a_signal_death_is_128_plus_the_signal() {
    let out = native().run(&sh("kill -9 $$"), LONG).unwrap();
    assert_eq!(out.exit_code, 128 + 9);
    assert!(!out.timed_out);
}

#[test]
fn a_missing_program_is_not_found() {
    let err = native()
        .run(&Command::new("/nonexistent/teamwire-test-program"), LONG)
        .unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::NotFound);
}

#[test]
fn timeout_reports_124_and_keeps_partial_output() {
    let started = SystemClock.monotonic();
    let out = native()
        .run(&sh("echo before; sleep 60"), TIMES_OUT)
        .unwrap();
    assert!(out.timed_out);
    assert_eq!(out.exit_code, 124);
    assert_eq!(out.stdout, b"before\n");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn timeout_kills_process_group() {
    let dir = tempfile::tempdir().unwrap();
    let pids = dir.path().join("pids");
    // The shell starts a grandchild, records both pids, and hangs.
    let script = format!("sleep 60 & echo $$ $! > {}; wait", pids.display());
    let out = native().run(&sh(&script), TIMES_OUT).unwrap();
    assert!(out.timed_out);
    assert_eq!(out.exit_code, 124);

    let text = std::fs::read_to_string(&pids).unwrap();
    let parsed: Vec<u32> = text
        .split_whitespace()
        .map(|p| p.parse().unwrap())
        .collect();
    assert_eq!(parsed.len(), 2, "pid file was {text:?}");
    // SIGKILL is delivered at once, but the kernel may take a moment to
    // reap the grandchild, which init adopts.
    let deadline = SystemClock.monotonic() + Duration::from_secs(5);
    while parsed.iter().any(|p| alive(*p)) && SystemClock.monotonic() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    for pid in parsed {
        assert!(!alive(pid), "process {pid} survived the timeout");
    }
}
