//! A harness that names the caller's session only through one lookup key,
//! as Codex does through `MCP_META_sessionId`, and that can run a function
//! when its startup dialog is answered, as a session-start hook would.
//! Everything else is the fake's.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, Harness, HarnessCaps, HarnessError, HarnessErrorCode, HarnessId, Host,
    LaunchSpec, Liveness, MemberLaunch, OneLine, OtelConfig, Pid, Place, Role, ScreenState,
    SessionId, SessionKind, StartupOutcome, TeamName, TrustPolicy, TurnEnd,
};
use teamwire_fakes::FakeHarness;

type StartupFn = Box<dyn Fn(&Place) + Send + Sync>;

pub(crate) struct HintHarness {
    inner: Arc<FakeHarness>,
    key: &'static str,
    on_startup: Option<StartupFn>,
    reads_contract: bool,
}

impl HintHarness {
    pub(crate) fn new(inner: Arc<FakeHarness>, key: &'static str) -> Self {
        Self {
            inner,
            key,
            on_startup: None,
            reads_contract: false,
        }
    }

    /// `launch_spec` reads the contract file, as Codex does, and fails when
    /// it is not there yet.
    pub(crate) fn reading_contract(mut self) -> Self {
        self.reads_contract = true;
        self
    }

    pub(crate) fn on_startup(mut self, f: impl Fn(&Place) + Send + Sync + 'static) -> Self {
        self.on_startup = Some(Box::new(f));
        self
    }
}

impl Harness for HintHarness {
    fn id(&self) -> HarnessId {
        self.inner.id()
    }
    fn capabilities(&self) -> HarnessCaps {
        self.inner.capabilities()
    }
    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        if self.reads_contract {
            std::fs::read_to_string(&m.contract_path).map_err(|e| {
                HarnessError::new(
                    HarnessErrorCode::Io,
                    format!(
                        "cannot read the contract {}: {e}",
                        m.contract_path.display()
                    ),
                )
            })?;
        }
        self.inner.launch_spec(m)
    }
    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        self.inner.resume_spec(m, s)
    }
    fn current_session(&self) -> Option<CallerSession> {
        None
    }
    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        Some(CallerSession {
            id: SessionId::parse(&get(self.key)?).unwrap(),
            kind: SessionKind::Interactive,
        })
    }
    fn session_cwd(&self, s: &SessionId) -> Result<Option<PathBuf>, HarnessError> {
        self.inner.session_cwd(s)
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
    fn line_pending(&self, tail: &str, line: &OneLine) -> bool {
        self.inner.line_pending(tail, line)
    }
    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        if let Some(f) = &self.on_startup {
            f(place);
        }
        self.inner.answer_startup(host, place, policy)
    }
    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)> {
        self.inner.otel_env(team, role, base)
    }
}
