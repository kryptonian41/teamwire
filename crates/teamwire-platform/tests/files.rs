//! `write_atomic`, `free_bytes`, `on_network_fs`, in temporary directories.

#![expect(
    clippy::unwrap_used,
    reason = "unwrap is allowed in #[test] functions only; the helper here is test code too"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use teamwire_core::PlatformErrorCode;
use teamwire_platform::native;

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn write_atomic_creates_a_file_and_leaves_no_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.json");
    native().write_atomic(&path, b"{}").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{}");
    assert_eq!(entries(dir.path()), ["a.json"]);
}

#[test]
fn write_atomic_replaces_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.json");
    native().write_atomic(&path, b"old").unwrap();
    native().write_atomic(&path, b"new and longer").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new and longer");
    assert_eq!(entries(dir.path()), ["a.json"]);
}

#[test]
fn write_atomic_handles_empty_and_large_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    native().write_atomic(&path, b"").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"");
    let big = vec![7u8; 3 << 20];
    native().write_atomic(&path, &big).unwrap();
    assert_eq!(fs::read(&path).unwrap(), big);
}

#[test]
fn write_atomic_keeps_the_mode_of_the_old_file_and_defaults_to_0644() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    native().write_atomic(&path, b"1").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    native().write_atomic(&path, b"2").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn a_failed_write_leaves_the_old_file_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.json");
    native().write_atomic(&path, b"old").unwrap();
    // A directory without write permission refuses the temporary file.
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let result = native().write_atomic(&path, b"new");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let err = result.unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::PermissionDenied);
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(entries(dir.path()), ["a.json"]);
}

#[test]
fn a_missing_directory_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let err = native()
        .write_atomic(&dir.path().join("no/such/dir/a"), b"x")
        .unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::NotFound);
}

#[test]
fn a_failed_rename_leaves_no_temporary_behind() {
    let dir = tempfile::tempdir().unwrap();
    // Renaming a file over a directory fails after the temporary is written.
    let target = dir.path().join("taken");
    fs::create_dir(&target).unwrap();
    assert!(native().write_atomic(&target, b"x").is_err());
    assert_eq!(entries(dir.path()), ["taken"]);
}

#[test]
fn a_path_without_a_file_name_is_an_error() {
    let err = native().write_atomic(Path::new("/"), b"x").unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::Io);
}

#[test]
fn free_bytes_is_positive_for_a_temporary_directory() {
    let dir = tempfile::tempdir().unwrap();
    assert!(native().free_bytes(dir.path()).unwrap() > 0);
}

#[test]
fn free_bytes_of_a_missing_path_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let err = native().free_bytes(&dir.path().join("nope")).unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::NotFound);
}

#[test]
fn a_temporary_directory_is_not_on_a_network_file_system() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!native().on_network_fs(dir.path()).unwrap());
}

#[test]
fn on_network_fs_of_a_missing_path_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let err = native()
        .on_network_fs(&dir.path().join("nope"))
        .unwrap_err();
    assert_eq!(err.code, PlatformErrorCode::NotFound);
}
