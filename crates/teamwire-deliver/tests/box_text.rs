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
    Address, CallerSession, DeliveryErrorCode, Harness, HarnessCaps, HarnessError, HarnessId, Host,
    HostCaps, HostError, HostId, Key, LaunchSpec, Liveness, MemberLaunch, Occupant, OneLine,
    OtelConfig, Pid, Place, PlaceState, Registry, Role, ScreenState, SessionId, SpawnRequest,
    StartupOutcome, TeamName, TrustPolicy, TurnEnd, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::{FakeHarness, FakeHost};
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

/// The fake host, except that its styled screen is `styled`: the box as it
/// looks once the style codes say its text is a dim placeholder.
struct StyledScreen {
    host: Arc<FakeHost>,
    styled: &'static str,
}

impl Host for StyledScreen {
    fn id(&self) -> HostId {
        self.host.id()
    }
    fn capabilities(&self) -> HostCaps {
        self.host.capabilities()
    }
    fn spawn(&self, req: &SpawnRequest, t: Duration) -> Result<Place, HostError> {
        self.host.spawn(req, t)
    }
    fn submit(&self, place: &Place, text: &OneLine, t: Duration) -> Result<(), HostError> {
        self.host.submit(place, text, t)
    }
    fn state(&self, place: &Place, t: Duration) -> Result<PlaceState, HostError> {
        self.host.state(place, t)
    }
    fn occupant(&self, place: &Place, t: Duration) -> Result<Occupant, HostError> {
        self.host.occupant(place, t)
    }
    fn read_tail(&self, place: &Place, lines: u16, t: Duration) -> Result<String, HostError> {
        self.host.read_tail(place, lines, t)
    }
    fn read_tail_styled(&self, _: &Place, _: u16, _: Duration) -> Result<String, HostError> {
        Ok(self.styled.to_owned())
    }
    fn send_keys(&self, place: &Place, keys: &[Key], t: Duration) -> Result<(), HostError> {
        self.host.send_keys(place, keys, t)
    }
    fn close(&self, place: &Place, expected: &Address, t: Duration) -> Result<(), HostError> {
        self.host.close(place, expected, t)
    }
    fn list(&self, t: Duration) -> Result<Vec<Place>, HostError> {
        self.host.list(t)
    }
    fn live_agent_names(&self, t: Duration) -> Result<Vec<String>, HostError> {
        self.host.live_agent_names(t)
    }
    fn place_env_var(&self) -> Option<&'static str> {
        self.host.place_env_var()
    }
    fn place_from_env(&self, read: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        self.host.place_from_env(read)
    }
}

#[test]
fn the_box_check_reads_the_styled_screen() {
    let mut rig = rig_reading_the_box();
    let mut registry = Registry::new();
    registry
        .add_harness(Arc::new(BoxReader(rig.harness.clone())))
        .unwrap();
    registry
        .add_host(Arc::new(StyledScreen {
            host: rig.host.clone(),
            styled: "ready\n> ",
        }))
        .unwrap();
    rig.registry = registry;
    let place = rig.healthy("lead-1", 100);
    // Plain text cannot tell a placeholder from typed text; the styles can.
    rig.host
        .set_screen(&place, "ready\n> close all three members");
    let outcome = deliver_line(&mut rig.ctx(), &Party::Lead, &one_line("hello", 80));
    assert_eq!(outcome, DeliveryOutcome::HandedOff);
    assert_eq!(rig.host.submitted(&place), ["hello"]);
}
