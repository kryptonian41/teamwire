use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use teamwire_core::{
    CallerSession, Command, Config, DeliveryErrorCode, Harness, HarnessCaps, HarnessError,
    HarnessId, LaunchSpec, Liveness, MemberLaunch, MemberState, NoHost, OtelConfig, Pid, Place,
    Platform, PlatformError, Registry, Role, RunOutput, ScreenState, SessionId, SleepHold,
    StartupOutcome, TeamName, Timestamp, TrustPolicy, TurnEnd,
};

use crate::testkit::{Fixture, at, new_member, new_team, role};
use crate::{Ctx, Party};

/// A harness that only knows its own id: enough to see which one the
/// registry hands back.
struct Stub(&'static str);

impl Harness for Stub {
    fn id(&self) -> HarnessId {
        HarnessId::from_static(self.0)
    }
    fn capabilities(&self) -> HarnessCaps {
        HarnessCaps {
            turn_end_hook: false,
            in_turn_hook: false,
            mcp_client: false,
            choose_session_id: false,
            can_push: false,
            session_env_via_config: false,
            watch_start: teamwire_core::harness::WatchStart::MonitoredCommand,
            load_skill: "",
            runs_afk: false,
        }
    }
    fn launch_spec(&self, _m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        Err(HarnessError::unsupported())
    }
    fn resume_spec(&self, _m: &MemberLaunch, _s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        Err(HarnessError::unsupported())
    }
    fn caller_session(&self, _get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        None
    }
    fn processes_for_session(
        &self,
        _s: &SessionId,
        _t: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        Ok(Vec::new())
    }
    fn is_alive(&self, _s: &SessionId, _t: Duration) -> Result<Liveness, HarnessError> {
        Ok(Liveness::Unknown)
    }
    fn parse_turn_end(&self, _stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        Err(HarnessError::unsupported())
    }
    fn hold_reply(&self, _note: &str) -> Vec<u8> {
        Vec::new()
    }
    fn screen_state(&self, _tail: &str) -> ScreenState {
        ScreenState::Unknown
    }
    fn answer_startup(
        &self,
        _host: &dyn teamwire_core::Host,
        _place: &Place,
        _policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        Ok(StartupOutcome::NoDialog)
    }
    fn otel_env(&self, _t: &TeamName, _r: &Role, _b: &OtelConfig) -> Vec<(String, String)> {
        Vec::new()
    }
}

struct StubPlatform;

impl Platform for StubPlatform {
    fn pid_alive(&self, _pid: Pid) -> bool {
        true
    }
    fn env_of(&self, _pid: Pid, _var: &str) -> Result<Option<String>, PlatformError> {
        Ok(None)
    }
    fn run(&self, _cmd: &Command, _timeout: Duration) -> Result<RunOutput, PlatformError> {
        Err(PlatformError::new(
            teamwire_core::PlatformErrorCode::Unsupported,
            "stub",
        ))
    }
    fn write_atomic(&self, _path: &Path, _bytes: &[u8]) -> Result<(), PlatformError> {
        Ok(())
    }
    fn sleep_hold(&self) -> Option<&dyn SleepHold> {
        None
    }
    fn free_bytes(&self, _path: &Path) -> Result<u64, PlatformError> {
        Ok(u64::MAX)
    }
    fn on_network_fs(&self, _path: &Path) -> Result<bool, PlatformError> {
        Ok(false)
    }
}

struct StubClock;

impl teamwire_core::Clock for StubClock {
    fn now(&self) -> Timestamp {
        at(0)
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "a test clock has to hand out an Instant, and only Instant::now makes one"
    )]
    fn monotonic(&self) -> Instant {
        Instant::now()
    }
}

fn registry(harnesses: &[&'static str]) -> Registry {
    let mut r = Registry::new();
    for h in harnesses {
        r.add_harness(Arc::new(Stub(h))).unwrap();
    }
    r.add_host(Arc::new(NoHost)).unwrap();
    r
}

/// The lead runs `fake-a` on host `none`; each named member runs `fake-b`.
fn team_with(f: &mut Fixture, members: &[&str]) {
    f.store
        .write(|tx| {
            let mut team = new_team();
            team.lead_harness = HarnessId::from_static("fake-a");
            team.lead_host = teamwire_core::HostId::from_static("none");
            tx.create_team(at(0), &team)?;
            for name in members {
                let mut m = new_member(name);
                m.harness = HarnessId::from_static("fake-b");
                m.host = teamwire_core::HostId::from_static("none");
                tx.insert_member(at(1), &m)?;
            }
            Ok(())
        })
        .unwrap();
}

fn ids(
    reg: &Registry,
    f: &mut Fixture,
    party: &Party,
) -> Result<(String, String), DeliveryErrorCode> {
    let config = Config::default();
    let home = f.home.path().to_path_buf();
    let ctx = Ctx {
        home: &home,
        store: &mut f.store,
        registry: reg,
        platform: &StubPlatform,
        clock: &StubClock,
        config: &config,
    };
    ctx.adapters(party)
        .map(|(h, host)| (h.id().to_string(), host.id().to_string()))
        .map_err(|e| e.code)
}

#[test]
fn lead_and_member_resolve_to_their_own_harnesses() {
    let mut f = crate::testkit::fixture();
    team_with(&mut f, &["api"]);
    let reg = registry(&["fake-a", "fake-b"]);
    assert_eq!(
        ids(&reg, &mut f, &Party::Lead),
        Ok(("fake-a".to_owned(), "none".to_owned()))
    );
    assert_eq!(
        ids(&reg, &mut f, &Party::Member(role("api"))),
        Ok(("fake-b".to_owned(), "none".to_owned()))
    );
}

#[test]
fn an_adapter_id_the_binary_lacks_is_unknown_adapter() {
    let mut f = crate::testkit::fixture();
    team_with(&mut f, &["api"]);
    // Only fake-a is registered: the member's fake-b is missing.
    let only_a = registry(&["fake-a"]);
    assert_eq!(
        ids(&only_a, &mut f, &Party::Member(role("api"))),
        Err(DeliveryErrorCode::UnknownAdapter)
    );
    assert!(ids(&only_a, &mut f, &Party::Lead).is_ok());
    // Only fake-b is registered: now the lead's harness is missing.
    let only_b = registry(&["fake-b"]);
    assert_eq!(
        ids(&only_b, &mut f, &Party::Lead),
        Err(DeliveryErrorCode::UnknownAdapter)
    );
    // A host id that is not registered.
    let mut no_host = Registry::new();
    no_host.add_harness(Arc::new(Stub("fake-a"))).unwrap();
    no_host.add_harness(Arc::new(Stub("fake-b"))).unwrap();
    assert_eq!(
        ids(&no_host, &mut f, &Party::Lead),
        Err(DeliveryErrorCode::UnknownAdapter)
    );
}

#[test]
fn a_removed_or_missing_member_is_recipient_removed() {
    let mut f = crate::testkit::fixture();
    team_with(&mut f, &["api", "web"]);
    f.store
        .write(|tx| {
            tx.remove_member(at(5), &role("web"), None, &json!({}))
                .map(|_| ())
        })
        .unwrap();
    let reg = registry(&["fake-a", "fake-b"]);
    assert!(ids(&reg, &mut f, &Party::Member(role("api"))).is_ok());
    assert_eq!(
        ids(&reg, &mut f, &Party::Member(role("web"))),
        Err(DeliveryErrorCode::RecipientRemoved)
    );
    assert_eq!(
        ids(&reg, &mut f, &Party::Member(role("ghost"))),
        Err(DeliveryErrorCode::RecipientRemoved)
    );
    let state = f
        .store
        .read(|tx| tx.member(&role("web")))
        .unwrap()
        .unwrap()
        .state;
    assert_eq!(state, MemberState::Removed);
}

#[test]
fn a_lead_without_a_team_row_or_an_unreadable_row_is_unknown_adapter() {
    let mut f = crate::testkit::fixture();
    let reg = registry(&["fake-a", "fake-b"]);
    assert_eq!(
        ids(&reg, &mut f, &Party::Lead),
        Err(DeliveryErrorCode::UnknownAdapter)
    );
    team_with(&mut f, &["api"]);
    // Damage the member's row so its adapters cannot be read back.
    f.store
        .write(|tx| {
            tx.connection().execute(
                "UPDATE members SET gen_started_at = 'garbage' WHERE role = 'api'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        ids(&reg, &mut f, &Party::Member(role("api"))),
        Err(DeliveryErrorCode::UnknownAdapter)
    );
}
