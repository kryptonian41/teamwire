//! `teamwire user-key` through the real binary (ADR 0040): the file's
//! format, its mode, that an existing key is kept, and that stdout carries
//! only the path. The dashboard relies on each of these.

#![cfg(test)]
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary and plant files it must not follow"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use teamwire_fakes::TestHome;

fn user_key(home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("user-key")
        .env_clear()
        .env("TEAMWIRE_HOME", home)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn is_key(text: &str) -> bool {
    text.len() == 65
        && text.ends_with('\n')
        && text[..64]
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[test]
fn a_missing_key_is_made_private_and_only_its_path_is_printed() {
    let home = TestHome::new().unwrap();
    let out = user_key(home.path());
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let path = home.path().join("user.key");
    assert!(path.is_absolute());
    assert_eq!(text(&out.stdout), format!("{}\n", path.display()));
    let key = std::fs::read_to_string(&path).unwrap();
    assert!(is_key(&key), "{key:?}");
    assert!(!text(&out.stdout).contains(key.trim()));
    assert!(!text(&out.stderr).contains(key.trim()));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn an_existing_key_is_kept() {
    let home = TestHome::new().unwrap();
    assert_eq!(user_key(home.path()).status.code(), Some(0));
    let path = home.path().join("user.key");
    let first = std::fs::read_to_string(&path).unwrap();
    let again = user_key(home.path());
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(text(&again.stdout), format!("{}\n", path.display()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
    // A key the user wrote by hand is kept as it is too.
    std::fs::write(&path, "my own key\n").unwrap();
    assert_eq!(user_key(home.path()).status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "my own key\n");
}

#[test]
fn an_empty_key_file_gets_a_key_and_mode_0600() {
    let home = TestHome::new().unwrap();
    let path = home.path().join("user.key");
    std::fs::write(&path, "\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(user_key(home.path()).status.code(), Some(0));
    assert!(is_key(&std::fs::read_to_string(&path).unwrap()));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_home_that_cannot_be_written_or_read_exits_6() {
    let home = TestHome::new().unwrap();
    // The home sits under a regular file, so it cannot be made.
    let file = home.path().join("plain");
    std::fs::write(&file, "x").unwrap();
    let out = user_key(&file.join("home"));
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "");
    // A directory at the key's path cannot be read as a key.
    std::fs::create_dir(home.path().join("user.key")).unwrap();
    let out = user_key(home.path());
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "");
}

#[test]
fn a_symlink_at_the_key_path_is_refused_and_not_followed() {
    let home = TestHome::new().unwrap();
    let target = home.path().join("elsewhere");
    std::os::unix::fs::symlink(&target, home.path().join("user.key")).unwrap();
    let out = user_key(home.path());
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    assert!(!target.exists());
}
