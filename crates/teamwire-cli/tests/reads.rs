//! `teamwire log` and `teamwire members` only read (WAT-11): run against a
//! database of another schema version, they refuse and change no byte.
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes a stand-in database file and starts the teamwire binary"
)]
#![expect(
    clippy::unwrap_used,
    reason = "a failure in this integration test fails the test"
)]

use std::process::Command;

fn run(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home)
        .args(args)
        .env_remove("TEAMWIRE_TOKEN")
        .env_remove("TEAMWIRE_TEAM")
        .env_remove("TEAMWIRE_ROLE")
        .output()
        .unwrap()
}

#[test]
fn log_and_members_never_migrate_an_older_database() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("teams").join("alpha");
    std::fs::create_dir_all(&dir).unwrap();
    // An empty file is an SQLite database at user_version 0: older than
    // this binary, as the first file of any older version would be.
    let db = dir.join("team.db");
    std::fs::write(&db, b"").unwrap();
    for args in [["log", "alpha"], ["members", "alpha"]] {
        let out = run(home.path(), &args);
        assert_eq!(out.status.code(), Some(6), "{args:?}: {out:?}");
        assert_eq!(std::fs::read(&db).unwrap(), b"", "{args:?} changed team.db");
    }
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["team.db"], "no side file was left");
}

#[test]
fn log_of_a_team_with_no_database_is_refused_and_creates_nothing() {
    let home = tempfile::tempdir().unwrap();
    let out = run(home.path(), &["log", "ghost"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(!home.path().join("teams").exists());
}
