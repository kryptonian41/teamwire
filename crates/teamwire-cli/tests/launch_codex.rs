//! `teamwire launch` of a Codex member through the real binary and the fake
//! herdr, whose argument rules are those of real herdr.

#![cfg(test)]
use std::io::Write as _;
use std::path::Path;
use std::process::Output;

use serde_json::{Value, json};
use teamwire_fakes::{FakeHerdrEnv, TestHome};

struct Rig {
    herdr: FakeHerdrEnv,
    home: TestHome,
    work: tempfile::TempDir,
}

#[expect(
    clippy::disallowed_methods,
    reason = "this test runs the real teamwire binary"
)]
fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    std::process::Command::new(program)
}

impl Rig {
    fn new() -> Self {
        Self {
            herdr: FakeHerdrEnv::new().unwrap(),
            home: TestHome::new().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    /// Runs `teamwire launch` with a cleared environment: the fakes' `PATH`,
    /// a temporary home, and `HERDR_ENV=1`.
    fn launch(&self, spec: &Value) -> Output {
        let mut child = command(env!("CARGO_BIN_EXE_teamwire"))
            .arg("launch")
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        {
            let mut stdin = stdin;
            stdin.write_all(spec.to_string().as_bytes()).unwrap();
        }
        child.wait_with_output().unwrap()
    }
}

fn spec(cwd: &Path) -> Value {
    json!({"team": "demo", "title": "Demo team", "harness": "codex", "children": [
        {"role": "a", "title": "Alpha", "mode": "pane", "cwd": cwd, "task": "look around"}]})
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_codex_member_starts_under_herdr() {
    let rig = Rig::new();
    let out = rig.launch(&spec(rig.work.path()));
    let (shown, err) = (text(&out.stdout), text(&out.stderr));
    assert!(!err.contains("invalid_agent_argument"), "{shown} / {err}");
    assert_eq!(out.status.code(), Some(0), "{shown} / {err}");
}
