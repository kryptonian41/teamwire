//! `lock_file`: the kernel's `flock` on a lock file, in temporary
//! directories. A second taker, a thread here, waits for the first.

#![expect(
    clippy::disallowed_methods,
    reason = "a test makes a file where a folder must go, and starts a holder process it then kills"
)]
#![expect(
    clippy::unwrap_used,
    reason = "unwrap is allowed in #[test] functions only; the helpers here are test code too"
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::{Clock, FileLock, PlatformError, PlatformErrorCode};
use teamwire_platform::{SystemClock, native};

/// A lock file whose folder does not exist yet.
fn lock_in(dir: &Path) -> PathBuf {
    dir.join("locks").join("a.lock")
}

#[test]
fn a_free_lock_is_taken_at_once_and_its_file_and_folder_are_made_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = lock_in(dir.path());
    let taken = native().lock_file(&path, Duration::ZERO);
    assert!(taken.is_ok(), "{taken:?}");
    assert!(path.is_file(), "the lock file was made");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"",
        "the lock file is never written"
    );
}

/// How long a test watches a taker that a held lock must hold back.
const WATCH: Duration = Duration::from_millis(300);

/// Takes the lock at `path` on a second thread while this thread holds it,
/// and checks the second taker gets it only once the first guard drops.
fn second_taker_waits_for_the_first(wait: Duration) {
    let dir = tempfile::tempdir().unwrap();
    let path = lock_in(dir.path());
    let platform = native();
    let first = platform.lock_file(&path, Duration::ZERO).unwrap();
    std::thread::scope(|s| {
        let second = s.spawn(|| platform.lock_file(&path, wait));
        std::thread::sleep(WATCH);
        assert!(
            !second.is_finished(),
            "the second taker got past a held lock (wait {wait:?})"
        );
        drop(first);
        let taken = second.join().unwrap();
        assert!(taken.is_ok(), "{taken:?}");
    });
}

#[test]
fn a_second_taker_waits_until_the_first_guard_drops() {
    second_taker_waits_for_the_first(Duration::from_secs(30));
}

#[test]
fn a_wait_too_large_for_the_clock_waits_as_long_as_the_holder_holds() {
    second_taker_waits_for_the_first(Duration::MAX);
}

/// Takes the held lock at `path` on a second thread with `wait`, and returns
/// what the second taker got and how long it took. Fails the test when the
/// second taker has not given up after 10 s. The thread is not scoped, so a
/// taker that never gives up cannot keep the test from failing.
fn take_while_held(path: &Path, wait: Duration) -> (Result<FileLock, PlatformError>, Duration) {
    let _first = native().lock_file(path, Duration::ZERO).unwrap();
    let owned = path.to_path_buf();
    let second = std::thread::spawn(move || {
        let started = SystemClock.monotonic();
        let taken = native().lock_file(&owned, wait);
        (taken, SystemClock.monotonic() - started)
    });
    for _ in 0..200 {
        if second.is_finished() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        second.is_finished(),
        "a taker with a {wait:?} wait was still waiting after 10 s"
    );
    second.join().unwrap()
}

#[test]
fn a_wait_past_its_limit_fails_with_the_timeout_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = lock_in(dir.path());
    let wait = Duration::from_millis(200);
    let (taken, waited) = take_while_held(&path, wait);
    let err = taken.unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::TimedOut, "{err}");
    assert!(
        waited >= wait,
        "gave up after {waited:?}, before the {wait:?} wait ran out"
    );
}

#[test]
fn a_zero_wait_on_a_busy_lock_fails_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = lock_in(dir.path());
    let (taken, waited) = take_while_held(&path, Duration::ZERO);
    let err = taken.unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::TimedOut, "{err}");
    assert!(waited < Duration::from_secs(1), "took {waited:?}");
}

#[test]
fn a_dropped_guard_frees_the_lock_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = lock_in(dir.path());
    let platform = native();
    drop(platform.lock_file(&path, Duration::ZERO).unwrap());
    let again = platform.lock_file(&path, Duration::ZERO);
    assert!(again.is_ok(), "{again:?}");
}

#[test]
fn a_lock_held_by_a_killed_process_is_taken_at_once() {
    use std::io::BufRead;
    let dir = tempfile::tempdir().unwrap();
    let path = lock_in(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // perl's flock is the kernel lock that `lock_file` takes.
    let mut holder = std::process::Command::new("/usr/bin/perl")
        .args([
            "-MFcntl=:flock",
            "-e",
            r#"$| = 1; open(my $f, ">>", $ARGV[0]) or die "open: $!"; flock($f, LOCK_EX) or die "flock: $!"; print "held\n"; sleep 60"#,
        ])
        .arg(&path)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut said = String::new();
    std::io::BufReader::new(holder.stdout.take().unwrap())
        .read_line(&mut said)
        .unwrap();
    assert_eq!(said, "held\n");
    let busy = native().lock_file(&path, Duration::ZERO).unwrap_err();
    assert_eq!(
        busy.code,
        PlatformErrorCode::TimedOut,
        "the child holds the lock: {busy}"
    );
    holder.kill().unwrap();
    holder.wait().unwrap();
    let taken = native().lock_file(&path, Duration::ZERO);
    assert!(taken.is_ok(), "{taken:?}");
}

/// Launch and close pass these texts on to the user as they are.
#[test]
fn a_folder_that_cannot_be_made_or_a_file_that_cannot_be_opened_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("locks");
    std::fs::File::create(&blocker).unwrap();
    let err = native()
        .lock_file(&blocker.join("a.lock"), Duration::ZERO)
        .unwrap_err();
    let want = format!("cannot make {}: ", blocker.display());
    assert!(err.detail.starts_with(&want), "{err}");

    let folder = dir.path().join("a.lock");
    std::fs::create_dir(&folder).unwrap();
    let err = native().lock_file(&folder, Duration::ZERO).unwrap_err();
    let want = format!("cannot open the lock {}: ", folder.display());
    assert!(err.detail.starts_with(&want), "{err}");
}
