//! Session files and state files: the folder, liveness and state from the
//! file the extension writes, and the context fill.

#![expect(
    clippy::unwrap_used,
    reason = "the test's helpers build fixtures from literals; a failure should fail the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes pi's files as pi and the extension would, not through the platform"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Fill, Harness, Liveness, Pid, PlaceState, SessionId, SessionUsage, Timestamp, TokenCounts,
    ToolRun,
};
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::{Pi, PiConfig};

const ID: &str = "01a1725a-5528-728a-b86b-775340828a01";
const STARTED_MS: i64 = 1_790_000_000_000;
const T: Duration = Duration::from_secs(5);

fn header(id: &str) -> String {
    format!(
        r#"{{"type":"session","version":3,"id":"{id}","timestamp":"2026-10-25T09:18:05.096Z","cwd":"/work/api"}}"#
    )
}

fn fill(tokens: u64) -> String {
    format!(
        r#"{{"type":"custom","customType":"teamwire.fill","data":{{"tokens":{tokens},"window":200000}},"id":"f{tokens}","parentId":"a1","timestamp":"2026-10-25T09:19:00.000Z"}}"#
    )
}

const MESSAGE: &str = r#"{"type":"message","id":"m1","parentId":"a1","timestamp":"2026-10-25T09:18:07.000Z","message":{"role":"user","content":"hello","timestamp":1}}"#;

struct Rig {
    _dir: tempfile::TempDir,
    agent_dir: PathBuf,
    platform: Arc<FakePlatform>,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let agent_dir = dir.path().join("agent");
        std::fs::create_dir_all(agent_dir.join("sessions/teamwire")).unwrap();
        Self {
            _dir: dir,
            agent_dir,
            platform: Arc::new(FakePlatform::new()),
        }
    }

    fn pi(&self) -> Pi {
        Pi::new(
            Arc::clone(&self.platform) as Arc<dyn teamwire_core::Platform>,
            PiConfig {
                agent_dir: self.agent_dir.clone(),
                pi_bin: "pi".into(),
                exclude_providers: Vec::new(),
            },
        )
    }

    /// Writes a session file named as pi names one.
    fn write(&self, stamp: &str, id: &str, lines: &[String]) -> PathBuf {
        let path = self
            .agent_dir
            .join(format!("sessions/teamwire/{stamp}_{id}.jsonl"));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    /// Writes a state file as the extension writes one.
    fn state(&self, id: &str, pid: u32, started_ms: i64, state: &str) {
        std::fs::write(
            self.agent_dir
                .join(format!("sessions/teamwire/{id}.teamwire.json")),
            format!(r#"{{"pid":{pid},"started_at_ms":{started_ms},"state":"{state}"}}"#),
        )
        .unwrap();
    }

    fn run(&self, pid: u32, started_ms: i64) {
        self.platform.set_alive(Pid::new(pid), true);
        self.platform.set_started(
            Pid::new(pid),
            Some(Timestamp::from_unix_millis(started_ms).unwrap()),
        );
    }
}

fn id() -> SessionId {
    SessionId::parse(ID).unwrap()
}

#[test]
fn the_folder_comes_from_the_header() {
    let rig = Rig::new();
    assert_eq!(rig.pi().session_cwd(&id()).unwrap(), None);
    rig.write("2026-10-25T09-18-05-096Z", ID, &[header(ID)]);
    assert_eq!(
        rig.pi().session_cwd(&id()).unwrap(),
        Some(Path::new("/work/api").to_path_buf())
    );
}

#[test]
fn a_member_before_its_first_message_is_alive_and_has_its_process() {
    // pi has written no session file yet; only the extension's state file.
    let rig = Rig::new();
    rig.state(ID, 77, STARTED_MS, "idle");
    rig.run(77, STARTED_MS);
    assert_eq!(
        rig.pi().is_alive(&id(), T).unwrap(),
        Liveness::Alive(PlaceState::Idle)
    );
    assert_eq!(
        rig.pi().processes_for_session(&id(), T).unwrap(),
        vec![Pid::new(77)]
    );
}

#[test]
fn the_recorded_state_is_the_place_state() {
    let rig = Rig::new();
    rig.run(77, STARTED_MS);
    for (state, want) in [
        ("idle", PlaceState::Idle),
        ("working", PlaceState::Working),
        ("blocked", PlaceState::Blocked),
        ("something-new", PlaceState::Unknown),
    ] {
        rig.state(ID, 77, STARTED_MS, state);
        assert_eq!(rig.pi().is_alive(&id(), T).unwrap(), Liveness::Alive(want));
    }
}

#[test]
fn a_process_that_ended_or_whose_pid_was_reused_is_dead() {
    let rig = Rig::new();
    rig.state(ID, 77, STARTED_MS, "working");
    assert_eq!(rig.pi().is_alive(&id(), T).unwrap(), Liveness::Dead);
    // Another process took the pid a minute later.
    rig.run(77, STARTED_MS + 60_000);
    assert_eq!(rig.pi().is_alive(&id(), T).unwrap(), Liveness::Dead);
    assert!(rig.pi().processes_for_session(&id(), T).unwrap().is_empty());
}

#[test]
fn a_session_its_process_left_is_dead_while_the_process_runs_on() {
    // A person ran /new in the pane: the same pi now runs another session.
    let rig = Rig::new();
    rig.state(ID, 77, STARTED_MS, "ended");
    rig.run(77, STARTED_MS);
    assert_eq!(rig.pi().is_alive(&id(), T).unwrap(), Liveness::Dead);
    assert!(rig.pi().processes_for_session(&id(), T).unwrap().is_empty());
}

#[test]
fn no_state_file_is_unknown() {
    let rig = Rig::new();
    assert_eq!(rig.pi().is_alive(&id(), T).unwrap(), Liveness::Unknown);
    rig.write(
        "2026-10-25T09-18-05-096Z",
        ID,
        &[header(ID), MESSAGE.to_owned()],
    );
    assert_eq!(rig.pi().is_alive(&id(), T).unwrap(), Liveness::Unknown);
}

#[test]
fn the_fill_is_the_newest_reading() {
    let rig = Rig::new();
    assert_eq!(rig.pi().context_fill(&id(), None).unwrap(), None);
    let path = rig.write(
        "2026-10-25T09-18-05-096Z",
        ID,
        &[header(ID), fill(30_000), MESSAGE.to_owned(), fill(91_000)],
    );
    let want = Some(Fill {
        tokens: 91_000,
        window: 200_000,
    });
    assert_eq!(rig.pi().context_fill(&id(), None).unwrap(), want);
    let other = SessionId::parse("never-ran").unwrap();
    assert_eq!(rig.pi().context_fill(&other, Some(&path)).unwrap(), want);
}

/// A pi session file in the shape pi 1.4 writes, its message text cut out.
const USAGE_FIXTURE: &str = include_str!("fixtures/session-usage.jsonl");

/// Every assistant message counts once, by its model. pi splits a response
/// into input, output, cache reads and cache writes, as Claude Code does.
#[test]
fn the_usage_sums_every_assistant_message_by_model() {
    let rig = Rig::new();
    assert_eq!(rig.pi().session_usage(&id(), None).unwrap(), None);
    let lines: Vec<String> = USAGE_FIXTURE.lines().map(str::to_owned).collect();
    let path = rig.write("2026-11-10T07-27-55-158Z", ID, &lines);
    let mut want = SessionUsage::new();
    want.insert(
        "claude-opus-5-5".to_owned(),
        TokenCounts {
            input: 4,
            output: 236,
            cache_read: 23_285,
            cache_creation: 24_976,
        },
    );
    want.insert(
        "gpt-5".to_owned(),
        TokenCounts {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_creation: 0,
        },
    );
    assert_eq!(
        rig.pi().session_usage(&id(), None).unwrap(),
        Some(want.clone())
    );
    let other = SessionId::parse("never-ran").unwrap();
    assert_eq!(
        rig.pi().session_usage(&other, Some(&path)).unwrap(),
        Some(want)
    );
}

#[test]
fn a_session_with_no_assistant_message_has_no_usage() {
    let rig = Rig::new();
    rig.write(
        "2026-10-25T09-18-05-096Z",
        ID,
        &[header(ID), MESSAGE.to_owned(), fill(30_000)],
    );
    assert_eq!(rig.pi().session_usage(&id(), None).unwrap(), None);
}

#[test]
fn another_sessions_state_is_not_read() {
    let rig = Rig::new();
    rig.state(
        "22222222-2222-4222-8222-222222222222",
        77,
        STARTED_MS,
        "idle",
    );
    rig.run(77, STARTED_MS);
    assert_eq!(rig.pi().is_alive(&id(), T).unwrap(), Liveness::Unknown);
}

/// The member's extension writes `tool` into the state file when a call
/// starts and drops it at the call's result. The tool comes from that field
/// while the recorded process runs the session.
#[test]
fn the_current_tool_comes_from_the_state_file() {
    let rig = Rig::new();
    rig.run(77, STARTED_MS);
    let file = rig
        .agent_dir
        .join(format!("sessions/teamwire/{ID}.teamwire.json"));
    let started = STARTED_MS + 60_000;
    std::fs::write(
        &file,
        format!(
            r#"{{"pid":77,"started_at_ms":{STARTED_MS},"state":"working","tool":{{"name":"bash","started_at_ms":{started}}}}}"#
        ),
    )
    .unwrap();
    assert_eq!(
        rig.pi().current_tool(&id(), None).unwrap(),
        Some(ToolRun {
            name: "bash".to_owned(),
            started_at: Timestamp::from_unix_millis(started).unwrap(),
        })
    );
    rig.state(ID, 77, STARTED_MS, "working");
    assert_eq!(rig.pi().current_tool(&id(), None).unwrap(), None);
}

/// A pi process that died mid-call never cleared its `tool`, so a state file
/// whose process is gone names no current call.
#[test]
fn a_dead_process_has_no_current_tool() {
    let rig = Rig::new();
    std::fs::write(
        rig.agent_dir
            .join(format!("sessions/teamwire/{ID}.teamwire.json")),
        format!(
            r#"{{"pid":77,"started_at_ms":{STARTED_MS},"state":"working","tool":{{"name":"bash","started_at_ms":{STARTED_MS}}}}}"#
        ),
    )
    .unwrap();
    assert_eq!(rig.pi().current_tool(&id(), None).unwrap(), None);
}
