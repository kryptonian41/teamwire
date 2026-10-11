//! Unit-level tests of the in-process fake `platform`.

use std::path::Path;
use std::time::Duration;

use teamwire_core::platform::ProcessRef;
use teamwire_core::{
    Command, Pid, Platform, PlatformError, PlatformErrorCode, ReleaseOutcome, RunOutput, Timestamp,
};
use teamwire_fakes::{CallKind, FakePlatform, HoldEvent};

fn pid(n: u32) -> Pid {
    Pid::new(n)
}

#[test]
fn a_reused_pid_reports_its_new_start_time() {
    let p = FakePlatform::new();
    let first = Timestamp::from_unix_millis(1_790_000_000_000).unwrap();
    let later = Timestamp::from_unix_millis(1_790_000_600_000).unwrap();
    // Unknown until set, and never for a dead pid.
    p.set_alive(pid(10), true);
    assert_eq!(p.process_started(pid(10)), None);
    p.set_started(pid(10), Some(first));
    assert_eq!(p.process_started(pid(10)), Some(first));
    let original = ProcessRef {
        pid: pid(10),
        started: Some(first),
    };
    assert!(p.process_alive(original));
    p.set_alive(pid(10), false);
    assert_eq!(p.process_started(pid(10)), None);
    assert!(!p.process_alive(original));
    // A new process gets the pid: alive, but not the recorded one.
    p.reuse_pid(pid(10), later);
    assert!(p.pid_alive(pid(10)));
    assert_eq!(p.process_started(pid(10)), Some(later));
    assert!(!p.process_alive(original));
    assert_eq!(p.log().count(CallKind::Write), 0);
}

#[test]
fn env_read_failure_is_reported_and_env_needs_a_live_pid() {
    let p = FakePlatform::new();
    p.set_alive(pid(10), true);
    p.set_env(pid(10), "HERDR_PANE_ID", "w1:p2");
    assert_eq!(
        p.env_of(pid(10), "HERDR_PANE_ID").unwrap().as_deref(),
        Some("w1:p2")
    );
    assert_eq!(p.env_of(pid(10), "OTHER").unwrap(), None);
    assert_eq!(
        p.env_of(pid(11), "X").unwrap_err().code,
        PlatformErrorCode::NotFound
    );
    p.fail_env_reads(Some(PlatformError::new(
        PlatformErrorCode::PermissionDenied,
        "no",
    )));
    assert_eq!(
        p.env_of(pid(10), "HERDR_PANE_ID").unwrap_err().code,
        PlatformErrorCode::PermissionDenied
    );
    p.fail_env_reads(None);
    assert!(p.env_of(pid(10), "HERDR_PANE_ID").is_ok());
}

#[test]
fn low_disk_and_network_fs_are_reported() {
    let p = FakePlatform::new();
    p.set_free_bytes(1000);
    assert_eq!(p.free_bytes(Path::new("/x")).unwrap(), 1000);
    p.fail_free_bytes(Some(PlatformError::new(PlatformErrorCode::Io, "stat")));
    assert!(p.free_bytes(Path::new("/x")).is_err());
    assert!(!p.on_network_fs(Path::new("/x")).unwrap());
    p.set_network_fs(true);
    assert!(p.on_network_fs(Path::new("/x")).unwrap());
}

#[test]
fn write_atomic_replaces_whole_file_and_full_disk_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = FakePlatform::new();
    let path = dir.path().join("a.json");
    p.write_atomic(&path, b"one").unwrap();
    p.write_atomic(&path, b"two").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"two");
    p.fail_writes(Some(PlatformError::new(PlatformErrorCode::NoSpace, "full")));
    assert_eq!(
        p.write_atomic(&path, b"three").unwrap_err().code,
        PlatformErrorCode::NoSpace
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"two");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(p.written(), [path.clone(), path]);
}

#[test]
fn write_into_a_missing_directory_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let err = FakePlatform::new()
        .write_atomic(&dir.path().join("no/such/f"), b"x")
        .unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::NotFound);
}

#[test]
fn run_replays_queued_results_and_records_commands() {
    let p = FakePlatform::new();
    p.queue_run(Ok(RunOutput {
        exit_code: RunOutput::TIMEOUT_EXIT,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: true,
    }));
    let cmd = Command::new("sleep").arg("9");
    assert!(p.run(&cmd, Duration::from_secs(1)).unwrap().timed_out);
    assert!(p.run(&cmd, Duration::from_secs(1)).unwrap().success());
    assert_eq!(p.runs().len(), 2);
    assert_eq!(p.log().count(CallKind::Write), 2);
}

#[test]
fn sleep_hold_records_start_and_release() {
    let p = FakePlatform::new();
    let hold = p.sleep_hold().unwrap();
    let lead = pid(42);
    assert!(hold.find_existing(lead).is_none());
    let info = hold.start(lead).unwrap();
    assert!(info.owned);
    assert_eq!(hold.find_existing(lead), Some(info));
    assert_eq!(hold.release(&info, lead).unwrap(), ReleaseOutcome::Released);
    assert_eq!(
        hold.release(&info, lead).unwrap(),
        ReleaseOutcome::AlreadyGone
    );
    assert_eq!(
        p.hold_events(),
        [
            HoldEvent::Started {
                lead,
                hold: info.pid
            },
            HoldEvent::Released {
                hold: info.pid,
                outcome: ReleaseOutcome::Released
            },
            HoldEvent::Released {
                hold: info.pid,
                outcome: ReleaseOutcome::AlreadyGone
            },
        ]
    );
}

#[test]
fn a_hold_the_user_started_is_never_released() {
    let p = FakePlatform::new();
    p.add_external_hold(pid(42), pid(500));
    let hold = p.sleep_hold().unwrap();
    let found = hold.find_existing(pid(42)).unwrap();
    assert!(!found.owned);
    assert_eq!(
        hold.release(&found, pid(42)).unwrap(),
        ReleaseOutcome::NotOwned
    );
    assert!(hold.find_existing(pid(42)).is_some());
}

#[test]
fn a_reused_hold_pid_is_left_alone() {
    let p = FakePlatform::new();
    let hold = p.sleep_hold().unwrap();
    let info = hold.start(pid(1)).unwrap();
    p.reuse_hold_pid(info.pid);
    assert_eq!(
        hold.release(&info, pid(1)).unwrap(),
        ReleaseOutcome::NotAHold
    );
    assert!(hold.find_existing(pid(1)).is_none());
}

#[test]
fn hold_start_can_fail_and_linux_has_no_hold() {
    let p = FakePlatform::new();
    p.fail_hold_start(Some(PlatformError::unsupported()));
    assert!(p.sleep_hold().unwrap().start(pid(1)).is_err());
    assert!(FakePlatform::without_sleep_hold().sleep_hold().is_none());
}
