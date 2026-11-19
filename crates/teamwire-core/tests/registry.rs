//! The adapter registry and the no-host adapter (ADR 0004).

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{CallerSession, HarnessId};
use teamwire_core::{
    DeliveryError, DeliveryErrorCode, Harness, HarnessCaps, HarnessError, Host, HostErrorCode,
    HostId, Key, LaunchSpec, Liveness, MemberLaunch, NoHost, OtelConfig, Pid, Place, Registry,
    RegistryError, Role, ScreenState, SessionId, SpawnRequest, SplitRequest, StartupOutcome,
    TeamName, TrustPolicy, TurnEnd, one_line,
};

#[derive(Debug)]
struct NamedHarness(&'static str);

impl Harness for NamedHarness {
    fn id(&self) -> HarnessId {
        HarnessId::from_static(self.0)
    }
    fn capabilities(&self) -> HarnessCaps {
        HarnessCaps {
            turn_end_hook: false,
            in_turn_hook: false,
            mcp_client: false,
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
        _host: &dyn Host,
        _place: &Place,
        _policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        Ok(StartupOutcome::NoDialog)
    }
    fn otel_env(&self, _t: &TeamName, _r: &Role, _b: &OtelConfig) -> Vec<(String, String)> {
        Vec::new()
    }
}

#[test]
fn unknown_and_duplicate_ids_refused() {
    let mut registry = Registry::new();
    registry
        .add_harness(Arc::new(NamedHarness("fake-a")))
        .unwrap();
    registry.add_host(Arc::new(NoHost)).unwrap();

    assert_eq!(
        registry
            .add_harness(Arc::new(NamedHarness("fake-a")))
            .unwrap_err(),
        RegistryError::Duplicate("fake-a".to_owned())
    );
    assert_eq!(
        registry.add_host(Arc::new(NoHost)).unwrap_err(),
        RegistryError::Duplicate("none".to_owned())
    );

    let unknown = HarnessId::parse("nonesuch").unwrap();
    let err = registry.harness(&unknown).err().unwrap();
    assert_eq!(err, RegistryError::UnknownHarness(unknown));
    assert_eq!(
        DeliveryError::from(err).code,
        DeliveryErrorCode::UnknownAdapter
    );

    let tmux = HostId::parse("tmux").unwrap();
    assert_eq!(
        registry.host(&tmux).err().unwrap(),
        RegistryError::UnknownHost(tmux)
    );

    let found = registry
        .harness(&HarnessId::parse("fake-a").unwrap())
        .unwrap();
    assert_eq!(found.id().as_str(), "fake-a");
    assert_eq!(
        registry
            .host(&HostId::parse("none").unwrap())
            .unwrap()
            .id()
            .as_str(),
        "none"
    );
}

#[test]
fn no_host_does_nothing_and_says_not_found() {
    let host = NoHost;
    let t = Duration::from_secs(1);
    let caps = host.capabilities();
    assert!(!caps.can_type && !caps.reports_state && !caps.knows_session_of_place);
    assert!(!caps.sets_env_at_spawn && !caps.guards_dialogs && !caps.splits_pane);
    assert_eq!(host.id().as_str(), NoHost::ID);

    let place = Place {
        host: host.id(),
        id: "p1".to_owned(),
    };
    let line = one_line("hello", 80);
    let spawn = SpawnRequest {
        name: teamwire_core::Address::parse("tw_tw_api").unwrap(),
        title: "api".to_owned(),
        team_title: "Demo team".to_owned(),
        generation: 1,
        cwd: "/tmp".into(),
        argv: vec!["true".to_owned()],
        env: Vec::new(),
    };
    assert_eq!(
        host.spawn(&spawn, t).unwrap_err().code,
        HostErrorCode::NotFound
    );
    let split = SplitRequest {
        label: "lead".to_owned(),
        cwd: "/tmp".into(),
        argv: vec!["true".to_owned()],
        env: Vec::new(),
    };
    assert_eq!(
        host.split_run(&place, &split, t).unwrap_err().code,
        HostErrorCode::Unsupported
    );
    assert_eq!(
        host.close_pane(&place, t).unwrap_err().code,
        HostErrorCode::Unsupported
    );
    assert_eq!(
        host.submit(&place, &line, t).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(
        host.send_keys(&place, &[Key::Enter], t).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(
        host.close(&place, &spawn.name, t).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(
        host.state(&place, t).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(
        host.occupant(&place, t).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert!(host.list(t).unwrap().is_empty());
    assert!(host.live_agent_names(t).unwrap().is_empty());
    assert!(host.place_env_var().is_none());
}

#[test]
fn harness_push_is_unsupported_by_default() {
    let h = NamedHarness("fake-a");
    let s = SessionId::parse("s1").unwrap();
    let err = h
        .push(&s, &one_line("x", 10), Duration::from_secs(1))
        .unwrap_err();
    assert_eq!(err.code, HostErrorCode::Unsupported);
}
