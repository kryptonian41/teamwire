//! `FakeHarness`: an in-process `Harness` with sessions, pids and liveness
//! in a map, and capabilities a test can switch (for example without a
//! turn-end hook).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::Key;
use teamwire_core::{
    CallerSession, Fill, Harness, HarnessCaps, HarnessError, HarnessErrorCode, HarnessId, Host,
    HostError, IdentityChannel, LaunchFile, LaunchSpec, LeadSetup, LeadStart, LeadStartSpec,
    Liveness, MemberLaunch, ModelCatalog, OneLine, OtelConfig, Pid, Place, Role, ScreenState,
    SessionId, SessionKind, StartupOutcome, TeamName, TrustPolicy, TurnEnd,
};

use crate::calls::{CallKind, CallLog, lock};
use crate::lead_setup::FakeLeadSetup;

mod lead_start;

/// The title of Claude Code's plain folder-trust dialog, as in Claude Code
/// 2.1.x. The dialog lists "No, exit" (preselected) and "Yes, I trust this
/// folder"; trusting takes Down, then Enter.
pub const PLAIN_TRUST_TEXT: &str =
    "Quick safety check: Is this a project you created or one you trust?";
/// The line naming the option that trusts the folder.
pub const TRUST_OPTION_TEXT: &str = "Yes, I trust this folder";
/// The extra text on the stronger dialog, which also grants what the folder's
/// own settings declare. It holds every escalation marker below. A harness
/// must never answer this dialog.
pub const ESCALATED_TRUST_TEXT: &str = "This folder adds a headersHelper that runs commands to mint credentials, and pre-approves hooks and extra directories. These will apply without asking.";
/// Markers of the plain dialog (`team-launch.py` `TRUST_MARKERS`).
pub const TRUST_MARKERS: [&str; 2] = ["Yes, I trust this folder", "Is this a project you created"];
/// Markers of the stronger dialog (`team-launch.py` `TRUST_ESCALATION_MARKERS`,
/// plus "pre-approves").
pub const TRUST_ESCALATION_MARKERS: [&str; 5] = [
    "will apply without asking",
    "runs commands to mint",
    "headersHelper",
    "This folder adds",
    "pre-approves",
];
/// What `fake-claude` prints when it is ready for input.
pub const PROMPT_TEXT: &str = "> ";

fn shows_trust_dialog(tail: &str) -> bool {
    TRUST_MARKERS
        .iter()
        .chain(&TRUST_ESCALATION_MARKERS)
        .any(|m| tail.contains(m))
}

#[derive(Debug, Clone)]
struct SessionRec {
    pids: Vec<Pid>,
    liveness: Liveness,
}

#[derive(Debug)]
struct State {
    caps: HarnessCaps,
    sessions: HashMap<SessionId, SessionRec>,
    caller: Option<CallerSession>,
    listing_fault: Option<HarnessError>,
    detects_pending: bool,
    pushed: Vec<(SessionId, String)>,
    cwds: HashMap<SessionId, PathBuf>,
    setup_missing: Option<String>,
    models: Option<Result<ModelCatalog, HarnessError>>,
    fills: HashMap<SessionId, Result<Fill, HarnessError>>,
    lead_starts: Vec<LeadStart>,
    lead_start_drops: Vec<String>,
    lead_start_warnings: Vec<String>,
    lead_start_fault: Option<HarnessError>,
}

/// An in-process `Harness`.
///
/// Faults: a background caller (`set_caller`), a dead session
/// (`add_session` with `Liveness::Dead`), two processes for one session
/// (`add_session` with two pids), a listing failure (`fail_listing`).
#[derive(Debug)]
pub struct FakeHarness {
    id: HarnessId,
    log: CallLog,
    state: Mutex<State>,
    lead: Option<FakeLeadSetup>,
}

impl FakeHarness {
    pub const ID: &'static str = "fake";

    /// Every capability on except `can_push` and `reads_input_box`.
    pub fn caps() -> HarnessCaps {
        HarnessCaps {
            turn_end_hook: true,
            in_turn_hook: true,
            mcp_client: true,
            choose_session_id: true,
            can_push: false,
            session_env_via_config: true,
            watch_start: teamwire_core::harness::WatchStart::MonitoredCommand,
            load_skill: "Load the fake teamwire skill.",
            reads_input_box: false,
            runs_afk: true,
        }
    }

    pub fn new() -> Self {
        Self::with_id(HarnessId::from_static(Self::ID))
    }

    /// For tests with two harnesses registered side by side (`fake-a`, `fake-b`).
    pub fn with_id(id: HarnessId) -> Self {
        Self {
            id,
            log: CallLog::default(),
            state: Mutex::new(State {
                caps: Self::caps(),
                sessions: HashMap::new(),
                caller: None,
                listing_fault: None,
                detects_pending: true,
                pushed: Vec::new(),
                cwds: HashMap::new(),
                setup_missing: None,
                models: None,
                fills: HashMap::new(),
                lead_starts: Vec::new(),
                lead_start_drops: Vec::new(),
                lead_start_warnings: Vec::new(),
                lead_start_fault: None,
            }),
            lead: None,
        }
    }

    /// A harness that can lead, through `lead`. Without this the fake has no
    /// lead setup, so install and doctor pass it by.
    #[must_use]
    pub fn with_lead_setup(mut self, lead: FakeLeadSetup) -> Self {
        self.lead = Some(lead);
        self
    }

    pub fn log(&self) -> &CallLog {
        &self.log
    }

    pub fn set_caps(&self, caps: HarnessCaps) {
        lock(&self.state).caps = caps;
    }

    pub fn add_session(&self, id: SessionId, pids: Vec<Pid>, liveness: Liveness) {
        lock(&self.state)
            .sessions
            .insert(id, SessionRec { pids, liveness });
    }

    pub fn set_liveness(&self, id: &SessionId, liveness: Liveness) {
        if let Some(rec) = lock(&self.state).sessions.get_mut(id) {
            rec.liveness = liveness;
        }
    }

    /// The session `caller_session` reports, whatever its lookup says.
    /// `None` means the caller has no session id in its environment.
    pub fn set_caller(&self, caller: Option<CallerSession>) {
        lock(&self.state).caller = caller;
    }

    pub fn set_interactive_caller(&self, id: SessionId) {
        self.set_caller(Some(CallerSession {
            id,
            kind: SessionKind::Interactive,
        }));
    }

    pub fn set_background_caller(&self, id: SessionId) {
        self.set_caller(Some(CallerSession {
            id,
            kind: SessionKind::Background,
        }));
    }

    /// What `lead_setup_missing` reports; `None`, the default, says the lead
    /// setup is loaded.
    pub fn set_lead_setup_missing(&self, why: Option<&str>) {
        lock(&self.state).setup_missing = why.map(str::to_owned);
    }

    /// Whether `line_pending` can tell (the default) or, like a harness that
    /// cannot read its screen, always answers `false`.
    pub fn set_detects_pending(&self, detects: bool) {
        lock(&self.state).detects_pending = detects;
    }

    /// `processes_for_session` and `is_alive` fail with `err` until cleared.
    pub fn fail_listing(&self, err: Option<HarnessError>) {
        lock(&self.state).listing_fault = err;
    }

    /// The folder `session_cwd` reports for `id`.
    pub fn set_session_cwd(&self, id: SessionId, cwd: impl Into<PathBuf>) {
        lock(&self.state).cwds.insert(id, cwd.into());
    }

    /// What `models` returns. Until this is called, `models` returns
    /// `Unsupported`, as an adapter without a model list does.
    pub fn set_models(&self, models: Result<ModelCatalog, HarnessError>) {
        lock(&self.state).models = Some(models);
    }

    /// What `context_fill` reports for `id`, whatever transcript it is
    /// handed. A session with none set has no reading (`Ok(None)`).
    pub fn set_fill(&self, id: SessionId, fill: Result<Fill, HarnessError>) {
        lock(&self.state).fills.insert(id, fill);
    }

    /// Lines `push` accepted, per session.
    pub fn pushed(&self) -> Vec<(SessionId, String)> {
        lock(&self.state).pushed.clone()
    }

    fn settings_json(m: &MemberLaunch, with_env: bool) -> Value {
        let hook = |event: &str| {
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": format!(
                        "{} hook {event} --team {} --role {}",
                        m.teamwire_bin.display(),
                        m.team.as_str(),
                        m.role.as_str()
                    ),
                }]
            }])
        };
        let mut doc = serde_json::Map::new();
        if with_env {
            let env: serde_json::Map<String, Value> = m
                .identity
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            doc.insert("env".to_owned(), Value::Object(env));
        }
        doc.insert(
            "hooks".to_owned(),
            json!({ "Stop": hook("claude-stop"), "PostToolUse": hook("claude-post-tool") }),
        );
        Value::Object(doc)
    }

    fn spec(
        &self,
        m: &MemberLaunch,
        resume: Option<&SessionId>,
    ) -> Result<LaunchSpec, HarnessError> {
        let caps = lock(&self.state).caps;
        let settings_path = m.member_dir.join("settings.json");
        let mut argv = vec!["fake-claude".to_owned()];
        match resume {
            Some(s) => argv.extend(["--resume".to_owned(), s.as_str().to_owned()]),
            None if caps.choose_session_id => {
                argv.extend(["--session-id".to_owned(), m.session_id.as_str().to_owned()]);
            }
            None => {}
        }
        argv.extend(["--settings".to_owned(), settings_path.display().to_string()]);
        let via_config = caps.session_env_via_config;
        let bytes = serde_json::to_vec_pretty(&Self::settings_json(m, via_config))
            .map_err(|e| HarnessError::new(HarnessErrorCode::Other, e.to_string()))?;
        Ok(LaunchSpec {
            argv,
            env: if via_config {
                Vec::new()
            } else {
                m.identity.clone()
            },
            files: vec![LaunchFile {
                path: settings_path,
                bytes,
                mode: 0o600,
            }],
            identity_channel: if via_config {
                IdentityChannel::HarnessConfig
            } else {
                IdentityChannel::HostEnv
            },
        })
    }
}

impl Default for FakeHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness for FakeHarness {
    fn id(&self) -> HarnessId {
        self.id.clone()
    }

    fn capabilities(&self) -> HarnessCaps {
        lock(&self.state).caps
    }

    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        self.log
            .record("launch_spec", CallKind::Read, m.address.as_str());
        self.spec(m, None)
    }

    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        self.log.record("resume_spec", CallKind::Read, s.as_str());
        self.spec(m, Some(s))
    }

    fn lead_setup(&self) -> Option<&dyn LeadSetup> {
        self.lead.as_ref().map(|l| l as &dyn LeadSetup)
    }

    fn lead_resume_argv(&self, s: &SessionId) -> Result<Vec<String>, HarnessError> {
        self.log
            .record("lead_resume_argv", CallKind::Read, s.as_str());
        Ok(vec![
            "fake-claude".to_owned(),
            "--resume".to_owned(),
            s.as_str().to_owned(),
        ])
    }

    fn lead_start(&self, req: &LeadStart) -> Result<LeadStartSpec, HarnessError> {
        self.build_lead_start(req)
    }

    fn session_cwd(&self, s: &SessionId) -> Result<Option<PathBuf>, HarnessError> {
        self.log.record("session_cwd", CallKind::Read, s.as_str());
        Ok(lock(&self.state).cwds.get(s).cloned())
    }

    fn caller_session(&self, _get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        self.log.record("caller_session", CallKind::Read, "");
        lock(&self.state).caller.clone()
    }

    fn lead_setup_missing(&self) -> Option<String> {
        lock(&self.state).setup_missing.clone()
    }

    fn processes_for_session(
        &self,
        s: &SessionId,
        _timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        self.log
            .record("processes_for_session", CallKind::Read, s.as_str());
        let state = lock(&self.state);
        if let Some(err) = &state.listing_fault {
            return Err(err.clone());
        }
        Ok(state
            .sessions
            .get(s)
            .map(|r| r.pids.clone())
            .unwrap_or_default())
    }

    fn is_alive(&self, s: &SessionId, _timeout: Duration) -> Result<Liveness, HarnessError> {
        self.log.record("is_alive", CallKind::Read, s.as_str());
        let state = lock(&self.state);
        if let Some(err) = &state.listing_fault {
            return Err(err.clone());
        }
        Ok(state.sessions.get(s).map_or(Liveness::Dead, |r| r.liveness))
    }

    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        self.log.record("parse_turn_end", CallKind::Read, "");
        let bad = |why: &str| HarnessError::new(HarnessErrorCode::BadInput, why.to_owned());
        let doc: Value = serde_json::from_slice(stdin).map_err(|e| bad(&e.to_string()))?;
        let text = |key: &str| doc.get(key).and_then(Value::as_str);
        let session = text("session_id").ok_or_else(|| bad("no session_id"))?;
        let session = SessionId::parse(session).map_err(|e| bad(&e.to_string()))?;
        Ok(TurnEnd {
            session,
            subagent: doc.get("agent_id").is_some_and(|v| !v.is_null()),
            hold_already_active: doc
                .get("stop_hook_active")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            final_message: text("last_assistant_message").map(str::to_owned),
            transcript: text("transcript_path").map(Into::into),
        })
    }

    fn hold_reply(&self, note: &str) -> Vec<u8> {
        self.log.record("hold_reply", CallKind::Read, "");
        serde_json::to_vec(&json!({ "decision": "block", "reason": note })).unwrap_or_default()
    }

    fn screen_state(&self, tail: &str) -> ScreenState {
        self.log.record("screen_state", CallKind::Read, "");
        if shows_trust_dialog(tail) {
            ScreenState::Dialog
        } else if tail.trim_end().ends_with('>') {
            ScreenState::InputPrompt
        } else {
            ScreenState::Unknown
        }
    }

    fn line_pending(&self, tail: &str, line: &OneLine) -> bool {
        self.log.record("line_pending", CallKind::Read, "");
        let squash = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
        let wanted: String = squash(line.as_str()).chars().take(40).collect();
        lock(&self.state).detects_pending
            && tail
                .lines()
                .rev()
                .find_map(|l| l.trim().strip_prefix('>').map(squash))
                .is_some_and(|text| text.contains(&wanted))
    }

    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        self.log
            .record("answer_startup", CallKind::Write, place.id.as_str());
        let timeout = Duration::from_secs(5);
        let tail = host.read_tail(place, 40, timeout)?;
        if TRUST_ESCALATION_MARKERS.iter().any(|m| tail.contains(m)) {
            return Ok(StartupOutcome::RefusedEscalated);
        }
        if !shows_trust_dialog(&tail) {
            return Ok(StartupOutcome::NoDialog);
        }
        match policy {
            TrustPolicy::Refuse => Err(HarnessError::new(
                HarnessErrorCode::Refused,
                "the spec does not accept the trust dialog",
            )),
            TrustPolicy::Accept => {
                // "No, exit" is preselected: Down moves to the trusting line.
                host.send_keys(place, &[Key::Down, Key::Enter], timeout)?;
                Ok(StartupOutcome::AnsweredPlainTrust)
            }
        }
    }

    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)> {
        self.log.record("otel_env", CallKind::Read, "");
        let Some(endpoint) = &base.endpoint else {
            return Vec::new();
        };
        vec![
            ("FAKE_OTEL_ENDPOINT".to_owned(), endpoint.clone()),
            (
                "FAKE_OTEL_RESOURCE".to_owned(),
                format!("team={},role={}", team.as_str(), role.as_str()),
            ),
        ]
    }

    fn models(&self, _timeout: Duration) -> Result<ModelCatalog, HarnessError> {
        self.log.record("models", CallKind::Read, "");
        lock(&self.state)
            .models
            .clone()
            .unwrap_or_else(|| Err(HarnessError::unsupported()))
    }

    fn context_fill(
        &self,
        s: &SessionId,
        _transcript: Option<&std::path::Path>,
    ) -> Result<Option<Fill>, HarnessError> {
        self.log.record("context_fill", CallKind::Read, s.as_str());
        lock(&self.state).fills.get(s).cloned().transpose()
    }

    fn push(&self, s: &SessionId, line: &OneLine, _timeout: Duration) -> Result<(), HostError> {
        self.log.record("push", CallKind::Write, s.as_str());
        let mut state = lock(&self.state);
        if !state.caps.can_push {
            return Err(HostError::unsupported());
        }
        state.pushed.push((s.clone(), line.as_str().to_owned()));
        Ok(())
    }
}
