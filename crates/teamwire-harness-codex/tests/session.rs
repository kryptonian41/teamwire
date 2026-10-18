//! Sessions and liveness over a temporary `codex_home` and a scripted `lsof`.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "tests read the recorded command by position; a missing one should fail the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes Codex's files as Codex would, not through the platform"
)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{Harness, HarnessErrorCode, Liveness, Pid, RunOutput, SessionId, SessionKind};
use teamwire_fakes::FakePlatform;
use teamwire_harness_codex::{CodexCli, CodexConfig};

const ID: &str = "019e0000-0000-7000-8000-000000000001";
const T: Duration = Duration::from_secs(5);

fn setup(home: &Path) -> (CodexCli, Arc<FakePlatform>) {
    let platform = Arc::new(FakePlatform::new());
    let codex = CodexCli::new(
        platform.clone(),
        CodexConfig {
            codex_home: home.to_path_buf(),
            codex_bin: "codex".into(),
        },
    );
    (codex, platform)
}

fn sid(s: &str) -> SessionId {
    SessionId::parse(s).unwrap()
}

fn rollout(home: &Path, id: &str, source: &str, cwd: &str) {
    let dir = home.join("sessions/2026/10/18");
    std::fs::create_dir_all(&dir).unwrap();
    let line = serde_json::json!({
        "timestamp": "2026-10-18T10:00:00.000Z", "type": "session_meta",
        "payload": {"id": id, "session_id": id, "cwd": cwd, "source": source},
    });
    std::fs::write(
        dir.join(format!("rollout-2026-10-18T10-00-00-{id}.jsonl")),
        format!("{line}\n{{\"type\":\"event_msg\"}}\n"),
    )
    .unwrap();
}

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let owned: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |k| owned.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
}

fn lock(home: &Path, id: &str) {
    let dir = home.join("thread-writer-locks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{id}.lock")), "").unwrap();
}

fn lsof(platform: &FakePlatform, code: i32, stdout: &str) {
    platform.queue_run(Ok(RunOutput {
        exit_code: code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out: false,
    }));
}

#[test]
fn session_cwd_reads_the_first_line_of_the_rollout() {
    let dir = tempfile::tempdir().unwrap();
    let (codex, _) = setup(dir.path());
    assert_eq!(codex.session_cwd(&sid(ID)).unwrap(), None);
    rollout(dir.path(), ID, "cli", "/work/tree");
    assert_eq!(
        codex.session_cwd(&sid(ID)).unwrap().unwrap().to_str(),
        Some("/work/tree")
    );
    assert_eq!(codex.session_cwd(&sid("../x")).unwrap(), None);
}

#[test]
fn the_caller_is_read_in_adr_order_and_only_cli_is_interactive() {
    let dir = tempfile::tempdir().unwrap();
    let (codex, _) = setup(dir.path());
    rollout(dir.path(), ID, "cli", "/w");
    let other = "019e0000-0000-7000-8000-000000000002";
    rollout(dir.path(), other, "exec", "/w");
    let got = codex
        .caller_session(&env(&[
            ("CODEX_SESSION_ID", ID),
            ("CODEX_THREAD_ID", other),
        ]))
        .unwrap();
    assert_eq!((got.id.as_str(), got.kind), (ID, SessionKind::Interactive));
    let got = codex
        .caller_session(&env(&[("CODEX_THREAD_ID", other)]))
        .unwrap();
    assert_eq!(got.kind, SessionKind::Background);
    let got = codex
        .caller_session(&env(&[("MCP_META_threadId", ID)]))
        .unwrap();
    assert_eq!(got.kind, SessionKind::Interactive);
    let unknown = codex
        .caller_session(&env(&[("MCP_META_sessionId", "no-rollout")]))
        .unwrap();
    assert_eq!(unknown.kind, SessionKind::Background);
    assert!(codex.caller_session(&env(&[])).is_none());
}

#[test]
fn a_lock_holder_means_alive_and_runs_lsof_under_the_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let (codex, platform) = setup(dir.path());
    lock(dir.path(), ID);
    platform.set_alive(Pid::new(4242), true);
    lsof(&platform, 0, "4242\n9999\n");
    assert_eq!(
        codex.processes_for_session(&sid(ID), T).unwrap(),
        [Pid::new(4242)]
    );
    let runs = platform.runs();
    assert_eq!(runs[0].0.program, "lsof");
    assert_eq!(runs[0].0.args[0], "-t");
    assert!(runs[0].0.args[1].ends_with(&format!("thread-writer-locks/{ID}.lock")));
    assert_eq!(runs[0].1, T);
    lsof(&platform, 0, "4242\n");
    assert!(matches!(
        codex.is_alive(&sid(ID), T).unwrap(),
        Liveness::Alive(_)
    ));
}

#[test]
fn no_holder_is_dead_only_with_a_rollout() {
    let dir = tempfile::tempdir().unwrap();
    let (codex, platform) = setup(dir.path());
    lock(dir.path(), ID);
    lsof(&platform, 1, "");
    assert_eq!(codex.is_alive(&sid(ID), T).unwrap(), Liveness::Unknown);
    rollout(dir.path(), ID, "cli", "/w");
    lsof(&platform, 1, "");
    assert_eq!(codex.is_alive(&sid(ID), T).unwrap(), Liveness::Dead);
    // No lock file: nothing to ask lsof about.
    assert_eq!(codex.is_alive(&sid("other"), T).unwrap(), Liveness::Unknown);
}

#[test]
fn a_failed_lsof_is_unknown_not_dead() {
    let dir = tempfile::tempdir().unwrap();
    let (codex, platform) = setup(dir.path());
    rollout(dir.path(), ID, "cli", "/w");
    lock(dir.path(), ID);
    lsof(&platform, 2, "");
    assert_eq!(codex.is_alive(&sid(ID), T).unwrap(), Liveness::Unknown);
    platform.queue_run(Ok(RunOutput {
        exit_code: 124,
        stdout: Vec::new(),
        stderr: Vec::new(),
        timed_out: true,
    }));
    assert_eq!(
        codex.processes_for_session(&sid(ID), T).unwrap_err().code,
        HarnessErrorCode::Timeout
    );
}

#[test]
fn the_lead_resumes_with_no_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let (codex, _) = setup(dir.path());
    assert_eq!(
        codex.lead_resume_argv(&sid(ID)).unwrap(),
        ["codex", "--no-daemon", "resume", ID]
    );
}
