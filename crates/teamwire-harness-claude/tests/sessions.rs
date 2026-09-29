//! Session identity and liveness: the session files first, then
//! `claude agents --json` with both row shapes.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

#[path = "support/env_platform.rs"]
mod env_platform;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    Harness, HarnessErrorCode, Liveness, Pid, PlaceState, Platform, PlatformError,
    PlatformErrorCode, RunOutput, SessionId, SessionKind,
};
use teamwire_fakes::{FakeHerdrEnv, FakePlatform};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

use env_platform::EnvPlatform;

const LIVE: u32 = 4242;
const GONE: u32 = 4343;

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn config(dir: &Path) -> ClaudeConfig {
    ClaudeConfig {
        claude_dir: dir.to_path_buf(),
        claude_json: dir.join("claude.json"),
        claude_bin: "claude".to_owned(),
    }
}

/// A `FakePlatform` where `LIVE` is running and `GONE` is not.
fn platform() -> Arc<FakePlatform> {
    let p = Arc::new(FakePlatform::new());
    p.set_alive(Pid::new(LIVE), true);
    p.set_alive(Pid::new(GONE), false);
    p
}

fn write_session(p: &FakePlatform, dir: &Path, pid: u32, session: &str, status: &str) {
    std::fs::create_dir_all(dir.join("sessions")).unwrap();
    let doc = json!({ "pid": pid, "sessionId": session, "status": status, "kind": "interactive" });
    p.write_atomic(
        &dir.join("sessions").join(format!("{pid}.json")),
        doc.to_string().as_bytes(),
    )
    .unwrap();
}

fn listing(p: &FakePlatform, rows: &serde_json::Value) {
    p.queue_run(Ok(RunOutput {
        exit_code: 0,
        stdout: rows.to_string().into_bytes(),
        stderr: Vec::new(),
        timed_out: false,
    }));
}

const T: Duration = Duration::from_secs(5);

#[test]
fn a_live_session_file_gives_the_state_its_status_names() {
    for (status, want) in [
        ("idle", PlaceState::Idle),
        ("busy", PlaceState::Working),
        ("waiting", PlaceState::Blocked),
        ("shell", PlaceState::Working),
        ("something-new", PlaceState::Unknown),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();
        write_session(&p, dir.path(), LIVE, "s-1", status);
        let claude = ClaudeCode::new(p.clone(), config(dir.path()));
        assert_eq!(
            claude.is_alive(&sid("s-1"), T).unwrap(),
            Liveness::Alive(want),
            "status {status}"
        );
        assert!(p.runs().is_empty(), "a live file needs no listing");
    }
}

#[test]
fn a_session_file_whose_process_is_gone_is_not_alive() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    write_session(&p, dir.path(), GONE, "s-1", "idle");
    listing(&p, &json!([]));
    let claude = ClaudeCode::new(p, config(dir.path()));
    assert_eq!(claude.is_alive(&sid("s-1"), T).unwrap(), Liveness::Dead);
}

#[test]
fn a_file_for_another_session_does_not_make_this_one_alive() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    write_session(&p, dir.path(), LIVE, "other", "idle");
    listing(&p, &json!([]));
    let claude = ClaudeCode::new(p, config(dir.path()));
    assert_eq!(claude.is_alive(&sid("s-1"), T).unwrap(), Liveness::Dead);
}

#[test]
fn a_bad_file_does_not_hide_a_good_one() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    std::fs::create_dir_all(dir.path().join("sessions")).unwrap();
    p.write_atomic(&dir.path().join("sessions/1.json"), b"{not json")
        .unwrap();
    write_session(&p, dir.path(), LIVE, "s-1", "busy");
    let claude = ClaudeCode::new(p, config(dir.path()));
    assert_eq!(
        claude.is_alive(&sid("s-1"), T).unwrap(),
        Liveness::Alive(PlaceState::Working)
    );
}

#[test]
fn a_failed_listing_is_unknown_never_dead() {
    let failures: Vec<(&str, Result<RunOutput, PlatformError>)> = vec![
        (
            "exit 1",
            Ok(RunOutput {
                exit_code: 1,
                stdout: Vec::new(),
                stderr: Vec::new(),
                timed_out: false,
            }),
        ),
        (
            "timed out",
            Ok(RunOutput {
                exit_code: 124,
                stdout: Vec::new(),
                stderr: Vec::new(),
                timed_out: true,
            }),
        ),
        (
            "not json",
            Ok(RunOutput {
                exit_code: 0,
                stdout: b"garbage".to_vec(),
                stderr: Vec::new(),
                timed_out: false,
            }),
        ),
        (
            "not a list",
            Ok(RunOutput {
                exit_code: 0,
                stdout: b"{}".to_vec(),
                stderr: Vec::new(),
                timed_out: false,
            }),
        ),
        (
            "no program",
            Err(PlatformError::new(PlatformErrorCode::NotFound, "claude")),
        ),
    ];
    for (what, result) in failures {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();
        // Even a file whose process is gone must not turn a failed listing into Dead.
        write_session(&p, dir.path(), GONE, "s-1", "idle");
        p.queue_run(result);
        let claude = ClaudeCode::new(p, config(dir.path()));
        assert_eq!(
            claude.is_alive(&sid("s-1"), T).unwrap(),
            Liveness::Unknown,
            "{what}"
        );
    }
}

#[test]
fn the_listing_runs_claude_agents_json_under_a_five_second_bound() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    listing(&p, &json!([]));
    let claude = ClaudeCode::new(p.clone(), config(dir.path()));
    claude
        .is_alive(&sid("s-1"), Duration::from_secs(60))
        .unwrap();
    let runs = p.runs();
    let (cmd, timeout) = runs.first().unwrap();
    assert_eq!(cmd.program, "claude");
    assert_eq!(cmd.args, ["agents", "--json"]);
    assert_eq!(*timeout, Duration::from_secs(5));
}

#[test]
fn a_caller_timeout_shorter_than_five_seconds_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    listing(&p, &json!([]));
    let claude = ClaudeCode::new(p.clone(), config(dir.path()));
    claude
        .is_alive(&sid("s-1"), Duration::from_secs(2))
        .unwrap();
    assert_eq!(p.runs().first().unwrap().1, Duration::from_secs(2));
}

#[test]
fn an_interactive_row_keeps_its_state_under_status() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    listing(
        &p,
        &json!([{ "sessionId": "s-1", "pid": LIVE, "status": "idle" }]),
    );
    let claude = ClaudeCode::new(p, config(dir.path()));
    assert_eq!(
        claude.is_alive(&sid("s-1"), T).unwrap(),
        Liveness::Alive(PlaceState::Idle)
    );
}

#[test]
fn a_background_row_keeps_its_state_under_state() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    listing(&p, &json!([{ "id": "s-1", "kind": "bg", "state": "busy" }]));
    let claude = ClaudeCode::new(p, config(dir.path()));
    assert_eq!(
        claude.is_alive(&sid("s-1"), T).unwrap(),
        Liveness::Alive(PlaceState::Working)
    );
}

#[test]
fn a_row_for_a_finished_job_or_a_gone_process_is_not_alive() {
    let dir = tempfile::tempdir().unwrap();
    for row in [
        json!({ "sessionId": "s-1", "kind": "bg", "state": "completed" }),
        json!({ "sessionId": "s-1", "pid": GONE, "status": "idle" }),
    ] {
        let p = platform();
        listing(&p, &json!([row]));
        let claude = ClaudeCode::new(p, config(dir.path()));
        assert_eq!(claude.is_alive(&sid("s-1"), T).unwrap(), Liveness::Dead);
    }
}

#[test]
fn processes_come_from_live_files_only() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    write_session(&p, dir.path(), LIVE, "s-1", "idle");
    write_session(&p, dir.path(), GONE, "s-1", "idle");
    let claude = ClaudeCode::new(p, config(dir.path()));
    assert_eq!(
        claude.processes_for_session(&sid("s-1"), T).unwrap(),
        [Pid::new(LIVE)]
    );
}

#[test]
fn processes_fall_back_to_the_listing_and_report_its_failure() {
    let dir = tempfile::tempdir().unwrap();
    let p = platform();
    listing(
        &p,
        &json!([{ "sessionId": "s-1", "pid": LIVE, "state": "busy" }]),
    );
    let claude = ClaudeCode::new(p.clone(), config(dir.path()));
    assert_eq!(
        claude.processes_for_session(&sid("s-1"), T).unwrap(),
        [Pid::new(LIVE)]
    );
    p.queue_run(Ok(RunOutput {
        exit_code: 124,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: true,
    }));
    let err = claude.processes_for_session(&sid("s-1"), T).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Timeout);
}

fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_owned())
    }
}

fn claude_for_env() -> ClaudeCode {
    ClaudeCode::new(Arc::new(FakePlatform::new()), config(Path::new("/nowhere")))
}

#[test]
fn current_session_reads_id_and_interactive_kind() {
    let got = claude_for_env().session_from_env(env_of(&[
        ("CLAUDE_CODE_SESSION_ID", "abc-123"),
        ("CLAUDE_CODE_SESSION_KIND", "interactive"),
    ]));
    let got = got.unwrap();
    assert_eq!(got.id, sid("abc-123"));
    assert_eq!(got.kind, SessionKind::Interactive);
}

#[test]
fn current_session_treats_bg_as_background() {
    let got = claude_for_env()
        .session_from_env(env_of(&[
            ("CLAUDE_CODE_SESSION_ID", "abc-123"),
            ("CLAUDE_CODE_SESSION_KIND", "bg"),
        ]))
        .unwrap();
    assert_eq!(got.kind, SessionKind::Background);
}

#[test]
fn current_session_without_a_kind_is_interactive() {
    let got = claude_for_env()
        .session_from_env(env_of(&[("CLAUDE_CODE_SESSION_ID", "abc-123")]))
        .unwrap();
    assert_eq!(got.kind, SessionKind::Interactive);
}

#[test]
fn current_session_without_an_id_is_none() {
    let claude = claude_for_env();
    assert!(claude.session_from_env(env_of(&[])).is_none());
    assert!(
        claude
            .session_from_env(env_of(&[("CLAUDE_CODE_SESSION_ID", "")]))
            .is_none()
    );
}

/// The real program shapes, through `fake-claude agents --json`: a session
/// file the fake wrote for a live pid, and a background row.
#[test]
fn fake_claude_listing_gives_both_row_shapes() {
    let env = FakeHerdrEnv::new().unwrap();
    let me = std::process::id();
    let bg = json!({
        "pid": me, "sessionId": "bg-1", "kind": "bg", "status": "busy",
        "cwd": "/tmp", "name": "bg", "startedAt": 1
    });
    env.add_session_file(me, &bg).unwrap();
    // No session file names a *live* file for "bg-2", so only the listing can.
    let dir = env.claude_dir();
    let listed = ClaudeCode::new(
        Arc::new(EnvPlatform::new(env.env())),
        ClaudeConfig {
            claude_dir: dir.join("elsewhere"),
            claude_json: dir.join("claude.json"),
            claude_bin: "claude".to_owned(),
        },
    );
    assert_eq!(
        listed.is_alive(&sid("bg-1"), T).unwrap(),
        Liveness::Alive(PlaceState::Working)
    );
    assert_eq!(listed.is_alive(&sid("bg-2"), T).unwrap(), Liveness::Dead);
}
