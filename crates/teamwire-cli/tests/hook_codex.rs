//! `teamwire hook session-start`, run as the real binary a Codex member's
//! `SessionStart` hook starts. The member row names `claude-code`, whose
//! input has the same `session_id` field the hook reads.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary as a harness would"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-verify/tests/support/fixture.rs"]
pub mod fixture;

use std::io::Write;
use std::process::{Command, Stdio};

use fixture::{Fixture, MemberSetup};
use teamwire_core::{MemberKind, Mode, Role, SessionId};

fn team() -> Fixture {
    let mut fx = Fixture::new();
    fx.add_member(&MemberSetup {
        role: "api",
        kind: MemberKind::Investigator,
        mode: Mode::Pane,
        check: None,
        owns: &[],
        base_sha: None,
        cwd: std::env::temp_dir(),
    });
    let api = Role::parse("api").unwrap();
    let hash = teamwire_msg::token_hash("secret");
    fx.store
        .write(|tx| tx.set_member_token_hash(&api, &hash))
        .unwrap();
    fx
}

/// Runs the hook with `env` and returns its exit code and standard output.
fn session_start(fx: &Fixture, env: &[(&str, &str)], session: &str) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(fx.home.path())
        .args(["hook", "session-start", "--team", "alpha", "--role", "api"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    let input = format!(r#"{{"session_id":"{session}","hook_event_name":"SessionStart"}}"#);
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

fn recorded(fx: &Fixture) -> Option<SessionId> {
    let api = Role::parse("api").unwrap();
    fx.store
        .read(|tx| tx.member(&api))
        .unwrap()
        .unwrap()
        .session_id
}

#[test]
fn the_token_from_the_environment_records_the_session() {
    let fx = team();
    let (code, out) = session_start(&fx, &[("TEAMWIRE_TOKEN", "secret")], "thread-1");
    assert_eq!((code, out.as_str()), (0, ""));
    assert_eq!(recorded(&fx), Some(SessionId::parse("thread-1").unwrap()));
}

#[test]
fn the_token_from_the_token_file_records_the_session() {
    let fx = team();
    let file = fx.home.path().join("token");
    std::fs::write(&file, "secret\n").unwrap();
    let path = file.display().to_string();
    let (code, _) = session_start(&fx, &[("TEAMWIRE_TOKEN_FILE", &path)], "thread-2");
    assert_eq!(code, 0);
    assert_eq!(recorded(&fx), Some(SessionId::parse("thread-2").unwrap()));
}

#[test]
fn a_wrong_token_exits_zero_and_records_nothing() {
    let fx = team();
    let (code, out) = session_start(&fx, &[("TEAMWIRE_TOKEN", "wrong")], "thread-1");
    assert_eq!((code, out.as_str()), (0, ""));
    assert_eq!(recorded(&fx), None);
}
