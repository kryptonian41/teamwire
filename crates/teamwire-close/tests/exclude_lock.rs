//! The lock close holds while it takes `info/exclude` lines back: the
//! kernel's `flock` on `$TEAMWIRE_HOME/locks/exclude-<sha256 of the
//! path>.lock`, the lock launch takes too. It ends with the holder's open
//! file, and the file is never deleted.
#![expect(
    clippy::disallowed_methods,
    reason = "a test seeds the exclude file, holds the lock through a file of its own, and starts a process that holds it"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rig::Rig;
use sha2::{Digest, Sha256};
use teamwire_close::{CloseOpts, CloseTarget, close};
use teamwire_core::MemberKind;

/// A team of one worktree member whose repository's exclude file holds a
/// line of the user's and `/TASK.md`, which the close takes back.
#[cfg(test)]
fn rig_with_task_md() -> (Rig, PathBuf) {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    let exclude = rig.repo.join(".git/info/exclude");
    std::fs::write(&exclude, "# mine\n/TASK.md\n").unwrap();
    (rig, exclude)
}

/// The lock file launch and close take for the rig's exclude file.
#[cfg(test)]
fn lock_path(rig: &Rig) -> PathBuf {
    let info = std::fs::canonicalize(rig.repo.join(".git/info")).unwrap();
    let digest = Sha256::digest(info.join("exclude").to_string_lossy().as_bytes());
    rig.home
        .path()
        .join("locks")
        .join(format!("exclude-{}.lock", hex::encode(digest)))
}

/// Holds the lock through an open file of the test's own, as another
/// teamwire process would. Dropping the file releases the lock.
#[cfg(test)]
fn hold(path: &Path) -> File {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap();
    file.try_lock().unwrap();
    file
}

#[test]
fn a_held_lock_holds_the_take_back_until_it_is_released_and_every_other_line_stays() {
    let (mut rig, exclude) = rig_with_task_md();
    let worktree = rig.member("api").cwd;
    let held = hold(&lock_path(&rig));
    std::thread::scope(|s| {
        let closing = s.spawn(|| close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default()));
        // The take-back comes right after the worktree goes. Wait up to the
        // suite's 60 s for that.
        for _ in 0..1200 {
            if !worktree.exists() || closing.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!worktree.exists(), "the worktree was removed");
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(
            std::fs::read_to_string(&exclude).unwrap(),
            "# mine\n/TASK.md\n",
            "rewritten under a held lock"
        );
        drop(held);
        let report = closing.join().unwrap();
        assert_eq!(report.stopped_at, None, "{report:?}");
    });
    assert_eq!(std::fs::read_to_string(&exclude).unwrap(), "# mine\n");
}

#[test]
fn a_lock_whose_holder_closed_its_file_is_taken_at_once() {
    let (mut rig, exclude) = rig_with_task_md();
    drop(hold(&lock_path(&rig)));
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(std::fs::read_to_string(&exclude).unwrap(), "# mine\n");
}

#[test]
fn a_lock_held_by_a_killed_process_is_taken_at_once() {
    use std::io::BufRead;
    let (mut rig, exclude) = rig_with_task_md();
    let lock = lock_path(&rig);
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    // perl's flock is the kernel lock that `File::try_lock` takes.
    let mut holder = std::process::Command::new("/usr/bin/perl")
        .args([
            "-MFcntl=:flock",
            "-e",
            r#"$| = 1; open(my $f, ">>", $ARGV[0]) or die "open: $!"; flock($f, LOCK_EX) or die "flock: $!"; print "held\n"; sleep 60"#,
        ])
        .arg(&lock)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut said = String::new();
    std::io::BufReader::new(holder.stdout.take().unwrap())
        .read_line(&mut said)
        .unwrap();
    assert_eq!(said, "held\n");
    assert!(
        File::open(&lock).unwrap().try_lock().is_err(),
        "the child holds the lock"
    );
    holder.kill().unwrap();
    holder.wait().unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(std::fs::read_to_string(&exclude).unwrap(), "# mine\n");
}

#[test]
fn the_lock_file_stays_after_the_close_and_no_lock_folder_or_pid_file_is_made() {
    let (mut rig, exclude) = rig_with_task_md();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert_eq!(std::fs::read_to_string(&exclude).unwrap(), "# mine\n");
    let lock = lock_path(&rig);
    assert!(
        File::open(&lock).unwrap().try_lock().is_ok(),
        "the close released the lock"
    );
    // The worktree lock close holds around `git worktree remove` sits beside it.
    let common = std::fs::canonicalize(rig.repo.join(".git")).unwrap();
    let digest = Sha256::digest(common.to_string_lossy().as_bytes());
    let worktree_lock = lock.with_file_name(format!("worktree-{}.lock", hex::encode(digest)));
    assert!(
        File::open(&worktree_lock).unwrap().try_lock().is_ok(),
        "the close released the worktree lock"
    );
    let mut left: Vec<PathBuf> = std::fs::read_dir(lock.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    left.sort();
    let mut want = vec![lock, worktree_lock];
    want.sort();
    assert_eq!(left, want);
}
