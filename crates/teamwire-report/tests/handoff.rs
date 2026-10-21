//! The in-turn note that a member's context passed its handoff point.
#![expect(
    clippy::unwrap_used,
    reason = "a failing unwrap in an integration test fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[path = "support/kit.rs"]
mod kit;
#[path = "../../teamwire-deliver/tests/support/members.rs"]
mod members;
#[path = "../../teamwire-deliver/tests/support/rig.rs"]
mod rig;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    CallerSession, Fill, Harness, HarnessCaps, HarnessError, HarnessId, Host, LaunchSpec, Liveness,
    MemberLaunch, MemberState, OtelConfig, Pid, Place, Role, ScreenState, SessionId,
    StartupOutcome, TeamName, TrustPolicy, TurnEnd,
};
use teamwire_fakes::FakeHarness;
use teamwire_report::handoff_note;

use kit::{at, sid, team};
use rig::Rig;

/// A harness whose context fill the test sets. Every other call is the
/// fake's, so the rig's own members still resolve.
struct Stub {
    fill: Mutex<Result<Option<Fill>, HarnessError>>,
    seen: Mutex<Vec<(SessionId, Option<std::path::PathBuf>)>>,
    inner: FakeHarness,
}

impl Stub {
    fn new() -> Self {
        Self {
            fill: Mutex::new(Ok(None)),
            seen: Mutex::new(Vec::new()),
            inner: FakeHarness::new(),
        }
    }

    fn set(&self, tokens: u64, window: u64) {
        *self.fill.lock().unwrap() = Ok(Some(Fill { tokens, window }));
    }
}

impl Harness for Stub {
    fn id(&self) -> HarnessId {
        self.inner.id()
    }
    fn capabilities(&self) -> HarnessCaps {
        self.inner.capabilities()
    }
    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        self.inner.launch_spec(m)
    }
    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        self.inner.resume_spec(m, s)
    }
    fn current_session(&self) -> Option<CallerSession> {
        self.inner.current_session()
    }
    fn processes_for_session(&self, s: &SessionId, t: Duration) -> Result<Vec<Pid>, HarnessError> {
        self.inner.processes_for_session(s, t)
    }
    fn is_alive(&self, s: &SessionId, t: Duration) -> Result<Liveness, HarnessError> {
        self.inner.is_alive(s, t)
    }
    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        self.inner.parse_turn_end(stdin)
    }
    fn hold_reply(&self, note: &str) -> Vec<u8> {
        self.inner.hold_reply(note)
    }
    fn screen_state(&self, tail: &str) -> ScreenState {
        self.inner.screen_state(tail)
    }
    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        self.inner.answer_startup(host, place, policy)
    }
    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)> {
        self.inner.otel_env(team, role, base)
    }
    fn context_fill(
        &self,
        s: &SessionId,
        transcript: Option<&Path>,
    ) -> Result<Option<Fill>, HarnessError> {
        self.seen
            .lock()
            .unwrap()
            .push((s.clone(), transcript.map(Path::to_path_buf)));
        self.fill.lock().unwrap().clone()
    }
}

/// The rig's member `api`, started, with `handoff_tokens` recorded when given.
fn started(rig: &mut Rig, api: &Role, handoff_tokens: Option<u64>) {
    rig.store
        .write(|tx| {
            tx.set_member_state(
                at(2),
                api,
                MemberState::Started,
                &json!({ "context_handoff_tokens": handoff_tokens }),
            )
        })
        .unwrap();
}

fn note(rig: &mut Rig, api: &Role, stub: &Stub) -> Option<String> {
    handoff_note(&mut rig.ctx(), api, stub, &sid("api-1"), None)
}

#[test]
fn below_the_point_there_is_no_note() {
    let (mut rig, api) = team();
    started(&mut rig, &api, None);
    let stub = Stub::new();
    stub.set(399_000, 1_000_000);
    assert_eq!(note(&mut rig, &api, &stub), None);
}

#[test]
fn past_the_point_one_note_with_the_numbers_and_the_path() {
    let (mut rig, api) = team();
    started(&mut rig, &api, None);
    let stub = Stub::new();
    stub.set(412_000, 1_000_000);
    let text = note(&mut rig, &api, &stub).unwrap();
    let path = rig.team_dir().join("reports/api.handoff.md");
    assert!(
        text.starts_with(
            "[teamwire] Your context holds 412k tokens, 41% of a 1000k window. Your handoff point is 400k."
        ),
        "{text}"
    );
    assert!(text.contains("`teamwire handoff --guide`"), "{text}");
    assert!(text.contains(&path.display().to_string()), "{text}");
    assert!(text.contains("starts with \"handoff:\""), "{text}");
    assert_eq!(note(&mut rig, &api, &stub), None, "once per generation");
}

#[test]
fn a_new_generation_past_the_point_is_told_again() {
    let (mut rig, api) = team();
    started(&mut rig, &api, None);
    let stub = Stub::new();
    stub.set(500_000, 1_000_000);
    assert!(note(&mut rig, &api, &stub).is_some());
    let place = rig.healthy("api-2", 200);
    rig.store
        .write(|tx| tx.record_handoff(at(9), &api, Some(&sid("api-2")), &place))
        .unwrap();
    let again = handoff_note(&mut rig.ctx(), &api, &stub, &sid("api-2"), None);
    assert!(again.is_some(), "generation 1 has not been told");
}

#[test]
fn a_recorded_handoff_point_moves_it() {
    let (mut rig, api) = team();
    started(&mut rig, &api, Some(100_000));
    let stub = Stub::new();
    stub.set(110_000, 1_000_000);
    let text = note(&mut rig, &api, &stub).unwrap();
    assert!(text.contains("Your handoff point is 100k."), "{text}");
}

#[test]
fn an_unknown_fill_or_a_failed_read_is_no_note() {
    let (mut rig, api) = team();
    started(&mut rig, &api, None);
    let stub = Stub::new();
    assert_eq!(note(&mut rig, &api, &stub), None, "Ok(None)");
    *stub.fill.lock().unwrap() = Err(HarnessError::unsupported());
    assert_eq!(note(&mut rig, &api, &stub), None, "Err");
}

#[test]
fn the_transcript_the_hook_was_given_reaches_the_harness() {
    let (mut rig, api) = team();
    started(&mut rig, &api, None);
    let stub = Stub::new();
    let path = Path::new("/t/s.jsonl");
    handoff_note(&mut rig.ctx(), &api, &stub, &sid("api-1"), Some(path));
    assert_eq!(
        stub.seen.lock().unwrap().as_slice(),
        [(sid("api-1"), Some(path.to_path_buf()))]
    );
}
