//! A pi lead (ADR 0030): how a session proves itself from the state file the
//! lead's extension writes, the folder and resume command of the lead, and
//! the refusal when the lead package is not loaded.

#![expect(
    clippy::unwrap_used,
    reason = "the test's helpers build fixtures from literals; a failure should fail the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes the state files as the extension would, not through the platform"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    CallerSession, Harness, Liveness, Pid, PlaceState, SessionId, SessionKind, Timestamp,
};
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::{Pi, PiConfig};

const LEAD: &str = "01a1725a-5528-728a-b86b-775340828a01";
const OTHER: &str = "01a1725a-5528-728a-b86b-775340828a02";
const STARTED_MS: i64 = 1_790_000_000_000;
const PI_PID: u32 = 4242;

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
                pi_bin: "/opt/pi/bin/pi".into(),
                exclude_providers: Vec::new(),
            },
        )
    }

    /// Writes a state file as the lead's extension writes one.
    fn lead_state(&self, id: &str, pid: u32, state: &str, mode: &str, session_file: &Path) {
        let doc = json!({
            "pid": pid,
            "started_at_ms": STARTED_MS,
            "state": state,
            "lead": true,
            "mode": mode,
            "session_file": session_file,
            "cwd": "/work",
        });
        std::fs::write(self.state_path(id), doc.to_string()).unwrap();
    }

    /// Writes a state file as a member's extension writes one.
    fn member_state(&self, id: &str, pid: u32) {
        let doc = json!({ "pid": pid, "started_at_ms": STARTED_MS, "state": "idle" });
        std::fs::write(self.state_path(id), doc.to_string()).unwrap();
    }

    fn state_path(&self, id: &str) -> PathBuf {
        self.agent_dir
            .join(format!("sessions/teamwire/{id}.teamwire.json"))
    }

    /// A lead session file in pi's own folder for the cwd.
    fn session_file(&self, id: &str) -> PathBuf {
        let dir = self.agent_dir.join("sessions/--work--");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("2026-10-30T10-00-00-000Z_{id}.jsonl"));
        std::fs::write(
            &path,
            format!(r#"{{"type":"session","version":3,"id":"{id}","cwd":"/work"}}"#) + "\n",
        )
        .unwrap();
        path
    }

    fn run(&self, pid: u32) {
        self.platform.set_alive(Pid::new(pid), true);
        self.platform.set_started(
            Pid::new(pid),
            Some(Timestamp::from_unix_millis(STARTED_MS).unwrap()),
        );
    }

    /// Makes pi the parent of this test process, as it is of the lead's MCP
    /// server.
    fn under_pi(&self) {
        self.platform
            .set_parent(Pid::new(std::process::id()), Pid::new(PI_PID), "pi");
    }
}

fn sid(id: &str) -> SessionId {
    SessionId::parse(id).unwrap()
}

fn interactive(id: &str) -> CallerSession {
    CallerSession {
        id: sid(id),
        kind: SessionKind::Interactive,
    }
}

fn env(id: &'static str) -> impl Fn(&str) -> Option<String> {
    move |name| (name == "PI_SESSION_ID").then(|| id.to_owned())
}

fn no_env(_: &str) -> Option<String> {
    None
}

/// The environment of the lead's MCP server, which the lead extension
/// registers with the session's id. `inherited` is a `PI_SESSION_ID` that
/// the pi process may itself have inherited.
fn server_env(
    id: &'static str,
    inherited: Option<&'static str>,
) -> impl Fn(&str) -> Option<String> {
    move |name| match name {
        "TEAMWIRE_PI_LEAD_SESSION" => Some(id.to_owned()),
        "PI_SESSION_ID" => inherited.map(str::to_owned),
        _ => None,
    }
}

#[test]
fn a_shell_command_of_the_lead_names_its_session_through_pi_session_id() {
    let rig = Rig::new();
    rig.run(PI_PID);
    rig.lead_state(LEAD, PI_PID, "working", "tui", &rig.session_file(LEAD));
    assert_eq!(rig.pi().caller_session(&env(LEAD)), Some(interactive(LEAD)));
}

#[test]
fn the_lead_mcp_server_proves_the_session_its_parent_pi_runs() {
    let rig = Rig::new();
    rig.run(PI_PID);
    rig.under_pi();
    rig.lead_state(LEAD, PI_PID, "idle", "tui", &rig.session_file(LEAD));
    // A `PI_SESSION_ID` that pi itself inherited does not decide.
    assert_eq!(
        rig.pi().caller_session(&server_env(LEAD, Some(OTHER))),
        Some(interactive(LEAD))
    );
}

#[test]
fn a_server_whose_parent_is_not_the_recorded_pi_proves_nothing() {
    let rig = Rig::new();
    rig.run(PI_PID);
    rig.lead_state(LEAD, PI_PID, "idle", "tui", &rig.session_file(LEAD));
    // No parent known, then a parent that is not the recorded pi.
    assert_eq!(rig.pi().caller_session(&server_env(LEAD, None)), None);
    rig.platform
        .set_parent(Pid::new(std::process::id()), Pid::new(PI_PID + 1), "pi");
    assert_eq!(rig.pi().caller_session(&server_env(LEAD, None)), None);
    // The server's session ended after a `/new`: the server is refused.
    rig.under_pi();
    rig.lead_state(LEAD, PI_PID, "ended", "tui", &rig.session_file(LEAD));
    assert_eq!(rig.pi().caller_session(&server_env(LEAD, None)), None);
}

#[test]
fn a_shell_command_needs_no_parent_lookup() {
    let rig = Rig::new();
    rig.run(PI_PID);
    rig.under_pi();
    rig.lead_state(LEAD, PI_PID, "idle", "tui", &rig.session_file(LEAD));
    assert_eq!(rig.pi().caller_session(&env(LEAD)), Some(interactive(LEAD)));
    assert_eq!(rig.platform.log().count_named("parent_of"), 0);
}

#[test]
fn only_the_terminal_ui_is_an_interactive_lead() {
    let rig = Rig::new();
    rig.run(PI_PID);
    for mode in ["print", "json", "rpc"] {
        rig.lead_state(LEAD, PI_PID, "idle", mode, &rig.session_file(LEAD));
        assert_eq!(
            rig.pi().caller_session(&env(LEAD)),
            Some(CallerSession {
                id: sid(LEAD),
                kind: SessionKind::Background
            }),
            "{mode}"
        );
    }
}

#[test]
fn a_member_an_ended_session_or_a_dead_pi_proves_no_lead() {
    let rig = Rig::new();
    let pi = rig.pi();
    // No state file: the lead package is not loaded.
    assert_eq!(pi.caller_session(&env(LEAD)), None);
    // A member's state file says nothing of a lead.
    rig.run(PI_PID);
    rig.member_state(LEAD, PI_PID);
    assert_eq!(pi.caller_session(&env(LEAD)), None);
    rig.under_pi();
    assert_eq!(pi.caller_session(&server_env(LEAD, None)), None);
    assert_eq!(pi.caller_session(&no_env), None);
    // The lead's session ended.
    rig.lead_state(LEAD, PI_PID, "ended", "tui", &rig.session_file(LEAD));
    assert_eq!(pi.caller_session(&env(LEAD)), None);
    // The recorded pi no longer runs.
    rig.lead_state(LEAD, PI_PID, "idle", "tui", &rig.session_file(LEAD));
    rig.platform.set_alive(Pid::new(PI_PID), false);
    assert_eq!(pi.caller_session(&env(LEAD)), None);
    assert_eq!(pi.caller_session(&server_env(LEAD, None)), None);
}

#[test]
fn the_lead_is_alive_and_its_folder_comes_from_its_own_session_file() {
    let rig = Rig::new();
    rig.run(PI_PID);
    rig.lead_state(LEAD, PI_PID, "working", "tui", &rig.session_file(LEAD));
    let pi = rig.pi();
    assert_eq!(
        pi.is_alive(&sid(LEAD), Duration::from_secs(5)).unwrap(),
        Liveness::Alive(PlaceState::Working)
    );
    assert_eq!(
        pi.processes_for_session(&sid(LEAD), Duration::from_secs(5))
            .unwrap(),
        vec![Pid::new(PI_PID)]
    );
    assert_eq!(
        pi.session_cwd(&sid(LEAD)).unwrap(),
        Some(PathBuf::from("/work"))
    );
}

#[test]
fn the_lead_resumes_by_id() {
    let rig = Rig::new();
    assert_eq!(
        rig.pi().lead_resume_argv(&sid(LEAD)).unwrap(),
        ["/opt/pi/bin/pi", "--session-id", LEAD]
    );
}

#[test]
fn a_lead_session_without_the_package_is_refused_with_the_install_command() {
    let rig = Rig::new();
    let why = rig.pi().lead_setup_missing().unwrap();
    assert!(why.contains("teamwire install --harness pi"), "{why}");
}

#[test]
fn a_pi_lead_starts_the_watch_with_the_extension_tool() {
    assert_eq!(
        Rig::new().pi().capabilities().watch_start,
        teamwire_core::harness::WatchStart::ExtensionTool
    );
}
