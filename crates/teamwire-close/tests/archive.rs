//! The archive is checked by content before anything is deleted.
#![expect(
    clippy::disallowed_methods,
    reason = "a test stands in for a disk that corrupts a file, or a member that writes late"
)]

#[path = "support/rig.rs"]
pub mod rig;

use std::path::PathBuf;

use rig::{Rig, entries};
use teamwire_close::{CloseOpts, CloseTarget, close, close_observed};
use teamwire_core::{MemberKind, Pid, PlaceState, TeamState};

/// Something that goes wrong in an archive directory.
type Damage = fn(&std::path::Path);

#[expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
fn one_archive(rig: &Rig) -> PathBuf {
    let all = entries(&rig.archive_root());
    assert_eq!(all.len(), 1, "{all:?}");
    all.first().unwrap().clone()
}

fn team() -> Rig {
    let mut rig = Rig::new();
    rig.add_worktree_member("api", MemberKind::Implementer);
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig.team_file("reports/api.md", "- abc1234 did it\n");
    rig.team_file("comms/m001.md", "hello\n");
    rig
}

/// Everything is where it was, and the team can be closed again.
fn assert_nothing_deleted(rig: &Rig, state: TeamState) {
    assert_eq!(rig.team().state, state);
    assert!(rig.team_dir().join("team.db").is_file());
    assert!(rig.team_dir().join("reports/api.md").is_file());
    assert!(
        rig.member("api").cwd.is_dir(),
        "the worktree is still there"
    );
    if state == TeamState::Open {
        for role in ["api", "scout"] {
            let host = &rig.host;
            let place = rig.place_of(role);
            assert_eq!(
                teamwire_core::Host::state(&**host, &place, std::time::Duration::ZERO),
                Ok(PlaceState::Idle),
                "{role}'s place is still open"
            );
        }
    }
}

#[test]
fn archive_hash_mismatch_stops_close() {
    // One byte of an archived file changes between the copy and the check
    // (a bad disk, a full disk). Each kind of damage stops the close before
    // any worktree, place, hold or state is touched.
    let damage: [(&str, Damage); 4] = [
        ("a copied file", |dir| {
            let path = dir.join("reports/api.md");
            std::fs::write(path, "- abc1234 did it!\n").unwrap();
        }),
        ("the database", |dir| {
            let path = dir.join("team.db");
            let mut bytes = std::fs::read(&path).unwrap();
            let last = bytes.last_mut().unwrap();
            *last ^= 0xff;
            std::fs::write(path, bytes).unwrap();
        }),
        ("a missing file", |dir| {
            std::fs::remove_file(dir.join("comms/m001.md")).unwrap();
        }),
        ("a file the sums do not list", |dir| {
            std::fs::write(dir.join("stray.txt"), "x\n").unwrap();
        }),
    ];
    for (what, hurt) in damage {
        let mut rig = team();
        let root = rig.archive_root();
        let report = close_observed(
            &mut rig.ctx(),
            CloseTarget::Team,
            CloseOpts::default(),
            &|step| {
                if step == "verify_archive" {
                    hurt(entries(&root).first().unwrap());
                }
            },
        );
        assert_eq!(
            report.stopped_at.as_deref(),
            Some("verify_archive"),
            "{what}: {report:?}"
        );
        assert!(!report.steps.last().unwrap().ok, "{what}");
        // Nothing after the check ran.
        let names: Vec<&str> = report.steps.iter().map(|s| s.step.as_str()).collect();
        assert_eq!(names, ["lock", "archive", "verify_archive"], "{what}");
        assert_nothing_deleted(&rig, TeamState::Open);
        assert!(rig.platform.inner.hold_events().is_empty(), "{what}");
        // The damaged archive is named, so the lead can look at it.
        assert_eq!(report.archive.unwrap(), one_archive(&rig), "{what}");
    }
}

#[test]
fn a_file_that_changes_after_it_was_archived_is_not_deleted() {
    // Something wrote to the team directory after the copy. Deleting the
    // directory would lose it, so the last check before the delete refuses.
    let mut rig = team();
    let late = rig.team_dir().join("reports/late.md");
    let report = close_observed(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts::default(),
        &|step| {
            if step == "delete_state" {
                std::fs::write(&late, "a report that came in late\n").unwrap();
            }
        },
    );
    assert_eq!(
        report.stopped_at.as_deref(),
        Some("delete_state"),
        "{report:?}"
    );
    assert!(
        report
            .steps
            .last()
            .unwrap()
            .detail
            .contains("is new since the archive")
    );
    assert!(late.is_file());
    assert_eq!(rig.team().state, TeamState::Closed);

    // Closing again takes a new archive that includes it, and deletes.
    let again = teamwire_close::close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(again.stopped_at, None, "{again:?}");
    let archive = again.archive.unwrap();
    assert_eq!(
        std::fs::read_to_string(archive.join("reports/late.md")).unwrap(),
        "a report that came in late\n"
    );
    assert!(!rig.team_dir().exists());
}

#[test]
fn an_archive_path_that_exists_is_never_written_into() {
    let mut rig = team();
    let opts = CloseOpts::default();
    // Whatever the archive directory is called, it is taken by another file.
    let first = teamwire_close::close(
        &mut rig.ctx(),
        CloseTarget::Team,
        CloseOpts {
            keep_state: true,
            ..opts.clone()
        },
    );
    let path = first.archive.unwrap();
    let before = std::fs::read(path.join("SHA256SUMS")).unwrap();
    let second = teamwire_close::close(&mut rig.ctx(), CloseTarget::Team, opts);
    assert_ne!(second.archive.unwrap(), path);
    assert_eq!(std::fs::read(path.join("SHA256SUMS")).unwrap(), before);
}

#[test]
fn a_close_leaves_no_team_directory_and_nothing_in_deleting() {
    let mut rig = team();
    let report = teamwire_close::close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(!rig.team_dir().exists());
    assert_eq!(
        entries(&rig.home.path().join("deleting")),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn a_close_removes_what_a_killed_close_left_in_deleting_unless_its_pid_is_alive() {
    let mut rig = team();
    let deleting = rig.home.path().join("deleting");
    // What a close killed between moving its team out of teams/ and removing
    // it leaves behind; one of the two closes is still running.
    for name in ["old.4000001", "busy.4000002"] {
        std::fs::create_dir_all(deleting.join(name).join("reports")).unwrap();
        std::fs::write(deleting.join(name).join("team.db"), "x").unwrap();
    }
    rig.platform.inner.set_alive(Pid::new(4_000_002), true);
    let report = teamwire_close::close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    assert!(!rig.team_dir().exists());
    assert_eq!(entries(&deleting), [deleting.join("busy.4000002")]);
}

#[test]
fn a_copy_that_stops_part_way_leaves_no_archive_and_the_rerun_leaves_one() {
    use std::os::unix::fs::PermissionsExt as _;
    let mut rig = team();
    let root = rig.archive_root();
    let unreadable = rig.team_dir().join("reports/api.md");
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at.as_deref(), Some("archive"), "{report:?}");
    assert_eq!(report.archive, None, "no half archive is reported");
    // What a kill leaves behind: a folder under a temporary name only.
    let left = entries(&root);
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(
        left.first()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".partial-alpha-"),
        "{left:?}"
    );
    // Another team's temporary folder is none of this close's business.
    let other = root.join(".partial-beta-2026-10-18");
    std::fs::create_dir_all(&other).unwrap();

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.stopped_at, None, "{report:?}");
    let all = entries(&root);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(other.is_dir());
    let mine = report.archive.unwrap();
    assert!(mine.join("SHA256SUMS").is_file());
    assert!(
        mine.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("alpha-20"),
        "{mine:?}"
    );
}
