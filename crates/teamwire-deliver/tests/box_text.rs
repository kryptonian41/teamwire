//! Text a person left in the input box is never sent with a delivered line,
//! on a host that guards dialogs too.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "support/rig.rs"]
mod rig;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, DeliveryErrorCode, Harness, HarnessCaps, HarnessError, HarnessId, Host,
    LaunchSpec, Liveness, MemberLaunch, OneLine, OtelConfig, Pid, Place, Registry, Role,
    ScreenState, SessionId, StartupOutcome, TeamName, TrustPolicy, TurnEnd, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::FakeHarness;
use teamwire_store::Party;

use rig::Rig;

/// The fake harness, except that it reads text after the fake's `> ` prompt
/// as text in the input box.
struct BoxReader(Arc<FakeHarness>);

impl Harness for BoxReader {
    fn id(&self) -> HarnessId {
        self.0.id()
    }
    fn capabilities(&self) -> HarnessCaps {
        self.0.capabilities()
    }
    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        self.0.launch_spec(m)
    }
    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        self.0.resume_spec(m, s)
    }
    fn current_session(&self) -> Option<CallerSession> {
        self.0.current_session()
    }
    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        self.0.caller_session(get)
    }
    fn session_cwd(&self, s: &SessionId) -> Result<Option<PathBuf>, HarnessError> {
        self.0.session_cwd(s)
    }
    fn processes_for_session(&self, s: &SessionId, t: Duration) -> Result<Vec<Pid>, HarnessError> {
        self.0.processes_for_session(s, t)
    }
    fn is_alive(&self, s: &SessionId, t: Duration) -> Result<Liveness, HarnessError> {
        self.0.is_alive(s, t)
    }
    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        self.0.parse_turn_end(stdin)
    }
    fn hold_reply(&self, note: &str) -> Vec<u8> {
        self.0.hold_reply(note)
    }
    fn screen_state(&self, tail: &str) -> ScreenState {
        self.0.screen_state(tail)
    }
    fn line_pending(&self, tail: &str, line: &OneLine) -> bool {
        self.0.line_pending(tail, line)
    }
    fn reads_input_box(&self) -> bool {
        true
    }
    fn box_holds_text(&self, tail: &str) -> bool {
        tail.lines()
            .last()
            .and_then(|l| l.strip_prefix("> "))
            .is_some_and(|text| !text.trim().is_empty())
    }
    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        self.0.answer_startup(host, place, policy)
    }
    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)> {
        self.0.otel_env(team, role, base)
    }
}

/// A rig whose lead harness reads the box, on the fake host, which guards
/// dialogs as herdr does.
fn rig_reading_the_box() -> Rig {
    let mut rig = Rig::new();
    let mut registry = Registry::new();
    registry
        .add_harness(Arc::new(BoxReader(rig.harness.clone())))
        .unwrap();
    registry.add_host(rig.host.clone()).unwrap();
    rig.registry = registry;
    assert!(rig.host.capabilities().guards_dialogs);
    rig
}

#[test]
fn a_guarded_host_does_not_type_after_text_left_in_the_box() {
    let mut rig = rig_reading_the_box();
    let place = rig.healthy("lead-1", 100);
    rig.host.set_screen(&place, "ready\n> fix the lin");
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert!(
        matches!(&outcome, DeliveryOutcome::Undelivered(e) if e.code == DeliveryErrorCode::NotAtInputPrompt),
        "{outcome:?}"
    );
    assert!(rig.host.submitted(&place).is_empty());
    assert_eq!(rig.host.log().count_named("read_tail"), 3);
}

#[test]
fn a_guarded_host_types_into_an_empty_box() {
    let mut rig = rig_reading_the_box();
    let place = rig.healthy("lead-1", 100);
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&place), ["hello"]);
}
