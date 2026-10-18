//! A member that has only `TEAMWIRE_TOKEN_FILE` (ADR 0023 point 4) is
//! identified by `done` and by `mcp --team`. `send` has its case in
//! `send.rs`.
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

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use fixture::{Fixture, MemberSetup};
use teamwire_core::{MemberKind, Mode, Role};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Team `alpha` with member `api`, and the file launch would have written.
fn team() -> (Fixture, String) {
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
    fx.store
        .write(|tx| tx.set_member_token_hash(&api, &teamwire_report::hash_token(TOKEN)))
        .unwrap();
    let file = fx.home.path().join("token");
    std::fs::write(&file, format!("{TOKEN}\n")).unwrap();
    (fx, file.display().to_string())
}

fn teamwire(fx: &Fixture, file: &str, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(fx.home.path())
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TEAMWIRE_TEAM", "alpha")
        .env("TEAMWIRE_ROLE", "api")
        .env("TEAMWIRE_TOKEN_FILE", file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn done_reads_the_token_file() {
    let (fx, file) = team();
    let out = teamwire(
        &fx,
        &file,
        &["done", "--status", "done", "--summary", "x"],
        "",
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("recorded completion 1"), "{text}");
}

#[test]
fn a_member_server_reads_the_token_file() {
    let (fx, file) = team();
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;
    let out = teamwire(
        &fx,
        &file,
        &["mcp", "--team", "alpha", "--role", "api"],
        &format!("{init}\n"),
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("\"serverInfo\""),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
