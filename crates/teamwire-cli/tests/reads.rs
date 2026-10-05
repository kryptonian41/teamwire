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
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
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

#[test]
fn every_command_refuses_without_a_home() {
    let commands: [&[&str]; 11] = [
        &[
            "send", "alpha", "--to", "api", "--type", "finding", "--text", "x",
        ],
        &["done", "--status", "done", "--summary", "x"],
        &["close", "alpha"],
        &["verify", "alpha"],
        &["export", "alpha"],
        &["metrics"],
        &["log", "alpha"],
        &["status"],
        &["watch", "alpha", "--once"],
        &["doctor"],
        &["launch", "--dry-run"],
    ];
    let cwd = tempfile::tempdir().unwrap();
    for args in commands {
        let mut command = Command::new(env!("CARGO_BIN_EXE_teamwire"));
        command.args(args).current_dir(cwd.path()).env_clear();
        if args[0] == "done" {
            // `done` asks who the caller is before it looks for the state.
            command
                .env("TEAMWIRE_TEAM", "alpha")
                .env("TEAMWIRE_ROLE", "api")
                .env("TEAMWIRE_TOKEN", "t");
        }
        let out = command.stdin(std::process::Stdio::null()).output().unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.contains("no state directory"), "{args:?}: {err}");
    }
    assert!(!cwd.path().join(".teamwire").exists());
}
