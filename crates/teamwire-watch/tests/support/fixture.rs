//! A team of fake members and a watch to poll them: the store, the in-process
//! fakes, a clock that only moves when a test says, and the watch's memory.
#![expect(
    clippy::unwrap_used,
    reason = "test support: a failure here fails the test"
)]

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    Actor, CallerSession, Clock, CompletionStatus, Config, DeliveryError, DeliveryErrorCode,
    DialogKind, Fill, FinalSource, Harness, HarnessCaps, HarnessError, HarnessId, Host, HostId,
    LaunchSpec, LeadRoute, Liveness, Member, MemberKind, MemberLaunch, MemberState, MessageType,
    Mode, Occupant, OtelConfig, PermissionRequest, Pid, Place, PlaceState, Recipient, Registry,
    ReportState, Role, Route, ScreenState, SessionId, StartupOutcome, TeamName, ToolRun,
    TrustPolicy, TurnEnd, Ulid,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_store::{Ctx, NewCompletion, NewMember, NewMessage, NewTeam, ResultOutcome, Store};
use teamwire_watch::WatchMemory;

/// The fake harness plus a context reading and a current tool call per
/// session, which the fake cannot give. Every other call goes to the fake.
#[derive(Debug)]
struct FillHarness {
    inner: Arc<FakeHarness>,
    fills: Arc<Mutex<HashMap<SessionId, Fill>>>,
    reads: Arc<Mutex<Vec<SessionId>>>,
    tools: Arc<Mutex<HashMap<SessionId, ToolRun>>>,
    /// The dialog kind every screen reads as, and how often one was read.
    kind: Arc<Mutex<(DialogKind, usize)>>,
}

impl Harness for FillHarness {
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
    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        self.inner.caller_session(get)
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
        _transcript: Option<&Path>,
    ) -> Result<Option<Fill>, HarnessError> {
        self.reads.lock().unwrap().push(s.clone());
        Ok(self.fills.lock().unwrap().get(s).copied())
    }
    fn current_tool(
        &self,
        s: &SessionId,
        _cwd: Option<&Path>,
    ) -> Result<Option<ToolRun>, HarnessError> {
        Ok(self.tools.lock().unwrap().get(s).cloned())
    }
    fn dialog_kind(&self, _tail: &str) -> DialogKind {
        let mut kind = self.kind.lock().unwrap();
        kind.1 += 1;
        kind.0
    }
}

pub const TEAM: &str = "alpha";
/// One poll's worth of time.
pub const POLL: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub struct Bed {
    pub home: TestHome,
    pub store: Store,
    pub clock: FakeClock,
    pub platform: FakePlatform,
    pub registry: Registry,
    pub config: Config,
    pub harness: Arc<FakeHarness>,
    pub host: Arc<FakeHost>,
    fills: Arc<Mutex<HashMap<SessionId, Fill>>>,
    reads: Arc<Mutex<Vec<SessionId>>>,
    tools: Arc<Mutex<HashMap<SessionId, ToolRun>>>,
    kind: Arc<Mutex<(DialogKind, usize)>>,
    pub memory: WatchMemory,
    places: HashMap<String, Place>,
}

pub fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

impl Bed {
    pub fn new() -> Self {
        Self::with_route(LeadRoute::Sink)
    }

    pub fn with_route(route: LeadRoute) -> Self {
        let home = TestHome::new().unwrap();
        let team = TeamName::parse(TEAM).unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        let clock = FakeClock::new();
        let now = clock.now();
        store
            .write(|tx| {
                tx.create_team(
                    now,
                    &NewTeam {
                        name: team.clone(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static(FakeHarness::ID),
                        lead_host: HostId::from_static(FakeHost::ID),
                        lead_route: route,
                        lead_session: None,
                    },
                )
                .map(|_| ())
            })
            .unwrap();
        let harness = Arc::new(FakeHarness::new());
        let host = Arc::new(FakeHost::new());
        let mut registry = Registry::new();
        let fills = Arc::new(Mutex::new(HashMap::new()));
        let reads = Arc::new(Mutex::new(Vec::new()));
        let tools = Arc::new(Mutex::new(HashMap::new()));
        let kind = Arc::new(Mutex::new((DialogKind::NoDialog, 0)));
        let wrapped = FillHarness {
            inner: harness.clone(),
            fills: fills.clone(),
            reads: reads.clone(),
            tools: tools.clone(),
            kind: kind.clone(),
        };
        registry.add_harness(Arc::new(wrapped)).unwrap();
        registry.add_host(host.clone()).unwrap();
        Self {
            home,
            store,
            clock,
            platform: FakePlatform::new(),
            registry,
            config: Config::default(),
            harness,
            host,
            fills,
            reads,
            tools,
            kind,
            memory: WatchMemory::default(),
            places: HashMap::new(),
        }
    }

    fn place(&self, name: &str) -> &Place {
        self.places
            .get(name)
            .unwrap_or_else(|| unreachable!("no member {name} was added"))
    }

    pub fn session(name: &str) -> SessionId {
        SessionId::parse(&format!("sess-{name}")).unwrap()
    }

    /// Adds a member that has started, is running and looks idle. Its
    /// deadline is 1800 s.
    pub fn add(&mut self, name: &str) -> Member {
        self.add_with_deadline(name, 1800)
    }

    pub fn add_with_deadline(&mut self, name: &str, deadline_s: u64) -> Member {
        self.insert(
            name,
            MemberKind::Implementer,
            deadline_s,
            true,
            None,
            &json!({}),
        )
    }

    /// Adds a started member whose `member.started` event was written by
    /// teamwire `version`.
    pub fn add_started_by(&mut self, name: &str, version: &str) -> Member {
        self.insert(
            name,
            MemberKind::Implementer,
            1800,
            true,
            Some(version),
            &json!({}),
        )
    }

    /// Adds a started member whose `member.started` event carries `data`.
    pub fn add_started_with(&mut self, name: &str, data: &serde_json::Value) -> Member {
        self.insert(name, MemberKind::Implementer, 1800, true, None, data)
    }

    /// Adds a started member of `kind`; `add` adds an implementer.
    pub fn add_kind(&mut self, name: &str, kind: MemberKind) -> Member {
        self.insert(name, kind, 1800, true, None, &json!({}))
    }

    /// Adds a member whose launch stopped in `creating`: no place, no
    /// session, never started.
    pub fn add_creating(&mut self, name: &str) {
        self.insert(name, MemberKind::Implementer, 1800, false, None, &json!({}));
    }

    /// Adds a started member whose `member.started` event says it got no
    /// first prompt (`"dispatch": false`).
    pub fn add_undispatched(&mut self, name: &str) -> Member {
        self.insert(
            name,
            MemberKind::Implementer,
            1800,
            true,
            None,
            &json!({ "dispatch": false }),
        )
    }

    fn insert(
        &mut self,
        name: &str,
        kind: MemberKind,
        deadline_s: u64,
        start: bool,
        by: Option<&str>,
        started_data: &serde_json::Value,
    ) -> Member {
        let role = role(name);
        let session = Self::session(name);
        let place = self.host.add_place(
            name,
            PlaceState::Idle,
            Occupant {
                session: None,
                pids: vec![],
            },
        );
        self.harness
            .add_session(session.clone(), vec![], Liveness::Alive(PlaceState::Idle));
        let now = self.clock.now();
        let new = NewMember {
            role: role.clone(),
            title: format!("the {name}"),
            kind,
            mode: Mode::Pane,
            harness: HarnessId::from_static(FakeHarness::ID),
            host: HostId::from_static(FakeHost::ID),
            cwd: self.home.path().to_path_buf(),
            repo: None,
            branch: Some("work".to_owned()),
            base: None,
            base_sha: None,
            check: None,
            owns: vec![],
            deadline_s,
        };
        if !start {
            self.store.write(|tx| tx.insert_member(now, &new)).unwrap();
            return self.member(name);
        }
        let team = TeamName::parse(TEAM).unwrap();
        let mut other = by.map(|v| {
            Store::open(self.home.path(), &team)
                .unwrap()
                .with_writer_version(v)
        });
        let store = other.as_mut().unwrap_or(&mut self.store);
        store
            .write(|tx| {
                tx.insert_member(now, &new)?;
                tx.set_member_place(&role, &place)?;
                tx.set_member_session(&role, &session)?;
                tx.set_member_state(now, &role, MemberState::Started, started_data)
            })
            .unwrap();
        self.places.insert(name.to_owned(), place);
        self.member(name)
    }

    pub fn member(&self, name: &str) -> Member {
        let role = role(name);
        self.store.read(|tx| tx.member(&role)).unwrap().unwrap()
    }

    /// When the member's current run started, as the store holds the text.
    pub fn started_at(&self, name: &str) -> String {
        self.store
            .read(|tx| {
                tx.connection()
                    .query_row(
                        "SELECT gen_started_at FROM members WHERE role = ?1",
                        [name],
                        |row| row.get(0),
                    )
                    .map_err(teamwire_store::StoreError::from)
            })
            .unwrap()
    }

    /// What the member's harness and host both report from now on.
    pub fn show(&self, name: &str, state: PlaceState) {
        self.harness
            .set_liveness(&Self::session(name), Liveness::Alive(state));
        self.host.set_state(self.place(name), state);
    }

    pub fn working(&self, name: &str) {
        self.show(name, PlaceState::Working);
    }

    pub fn idle(&self, name: &str) {
        self.show(name, PlaceState::Idle);
    }

    pub fn blocked(&self, name: &str) {
        self.show(name, PlaceState::Blocked);
    }

    /// The harness no longer knows the member's session. The host's place
    /// stays, so the host's listing is not empty.
    pub fn vanish(&self, name: &str) {
        self.harness
            .set_liveness(&Self::session(name), Liveness::Dead);
    }

    /// What the harness reads as the member's context fill.
    pub fn fill(&self, name: &str, tokens: u64, window: u64) {
        self.fills
            .lock()
            .unwrap()
            .insert(Self::session(name), Fill { tokens, window });
    }

    /// What the harness reads as the tool call the member's session runs.
    pub fn tool(&self, name: &str, run: ToolRun) {
        self.tools.lock().unwrap().insert(Self::session(name), run);
    }

    /// The dialog kind the harness reads on every screen from now on. The
    /// member's screen gets one line, so a read of it succeeds.
    pub fn screen_kind(&self, name: &str, kind: DialogKind) {
        self.host.set_screen(self.place(name), "a dialog\n");
        self.kind.lock().unwrap().0 = kind;
    }

    /// How many screens the harness was asked to name a dialog in.
    pub fn kind_reads(&self) -> usize {
        self.kind.lock().unwrap().1
    }

    /// Records a pending permission dialog of the member's current session
    /// for `command`, as its hook would, open for `open_for`.
    pub fn dialog(&mut self, name: &str, command: &str, open_for: Duration) -> Ulid {
        let member = self.member(name);
        let now = self.clock.now();
        let request = PermissionRequest {
            session: Self::session(name),
            subagent: false,
            tool: "Bash".to_owned(),
            summary: command.to_owned(),
            input: Some(json!({ "command": command })),
            why_not: None,
        };
        let expires = now.checked_add(open_for).unwrap();
        self.store
            .write(|tx| tx.open_dialog(now, &member.role, member.generation, &request, expires))
            .unwrap()
    }

    /// How many times the harness was asked for the member's context fill.
    pub fn fill_reads(&self, name: &str) -> usize {
        let session = Self::session(name);
        self.reads
            .lock()
            .unwrap()
            .iter()
            .filter(|s| **s == session)
            .count()
    }

    /// Marks a started member as failed; it keeps its recorded session.
    pub fn fail(&mut self, name: &str) {
        let (now, role) = (self.clock.now(), role(name));
        self.store
            .write(|tx| tx.set_member_state(now, &role, MemberState::Failed, &json!({})))
            .unwrap();
    }

    pub fn age(&self, by: Duration) {
        self.clock.advance(by);
    }

    /// One poll now, at the current time.
    pub fn poll(&mut self) -> Option<String> {
        let mut ctx = Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        };
        teamwire_watch::poll(&mut ctx, &mut self.memory).map(teamwire_core::OneLine::into_string)
    }

    /// Starts a watch as live process `pid`, which the fake platform reports alive.
    pub fn start_watch(&mut self, pid: u32) {
        let pid = teamwire_core::Pid::new(pid);
        self.platform.set_alive(pid, true);
        let mut ctx = Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        };
        teamwire_watch::start(&mut ctx, &mut WatchMemory::default(), pid, "testhost").unwrap();
    }

    /// One poll interval later, one poll.
    pub fn tick(&mut self) -> Option<String> {
        self.clock.advance(POLL);
        self.poll()
    }

    /// `n` ticks; the lines that were printed.
    pub fn ticks(&mut self, n: usize) -> Vec<String> {
        (0..n).filter_map(|_| self.tick()).collect()
    }

    /// Stores a completion for the member's current generation.
    pub fn complete(&mut self, name: &str, status: CompletionStatus) -> Ulid {
        self.complete_with(name, status, &format!("{name} finished"))
    }

    /// `complete` with a chosen summary, as a member's own `done` would
    /// record it.
    pub fn complete_with(&mut self, name: &str, status: CompletionStatus, summary: &str) -> Ulid {
        let member = self.member(name);
        let now = self.clock.now();
        let new = NewCompletion {
            role: member.role.clone(),
            generation: member.generation,
            status,
            summary: summary.to_owned(),
            final_message: None,
            final_source: FinalSource::Hook,
        };
        self.store
            .write(|tx| tx.insert_completion(now, &new).map(|c| c.ulid))
            .unwrap()
    }

    /// Ends the delivery of a completion as the hook would.
    pub fn finish(
        &mut self,
        completion: &Ulid,
        state: ReportState,
        code: Option<DeliveryErrorCode>,
    ) {
        let now = self.clock.now();
        let outcome = ResultOutcome {
            state,
            route: (state == ReportState::Delivered).then_some(Route::HostTyping),
            error: code.map(|c| DeliveryError::new(c, "test")),
            result_sha256: None,
        };
        self.store
            .write(|tx| {
                tx.claim_completion_for_delivery(now, completion, Duration::from_secs(90))?;
                tx.finish_completion_delivery(now, completion, &outcome)
                    .map(|_| ())
            })
            .unwrap();
    }

    /// A member that finished and whose result reached the lead.
    pub fn delivered(&mut self, name: &str, status: CompletionStatus) -> Ulid {
        let id = self.complete(name, status);
        self.finish(&id, ReportState::Delivered, None);
        id
    }

    /// A question from `name` to the lead, delivered as `state`.
    pub fn question(&mut self, name: &str, state: teamwire_core::DeliveryState) -> String {
        self.question_from(Actor::Member(role(name)), state)
    }

    /// A question to the lead from `sender`, which may be an agent of
    /// another team, left in `state`.
    pub fn question_from(&mut self, sender: Actor, state: teamwire_core::DeliveryState) -> String {
        let now = self.clock.now();
        let new = NewMessage {
            sender,
            kind: MessageType::Question,
            re: None,
            commit: None,
            body: "which branch?".to_owned(),
            client_key: None,
            recipients: vec![Recipient::Lead],
        };
        self.store
            .write(|tx| {
                let stored = tx.insert_message(now, &new)?;
                tx.set_delivery_state(
                    now,
                    &stored.message.ulid,
                    &Recipient::Lead,
                    &teamwire_store::DeliveryUpdate {
                        state,
                        route: None,
                        error: None,
                        attempts: 1,
                    },
                )?;
                Ok(stored.message.display_id.to_string())
            })
            .unwrap()
    }

    /// A message from the lead to member `to`, which the host took (`handed_off`).
    pub fn send(&mut self, kind: MessageType, to: &str, re: Option<Ulid>) -> Ulid {
        // Longer than the typed line's excerpt, so the line cannot carry the
        // whole text and the delivery stays unread until it is acked.
        self.send_body(kind, to, re, &"text ".repeat(80))
    }

    /// `send` with a chosen body.
    pub fn send_body(&mut self, kind: MessageType, to: &str, re: Option<Ulid>, body: &str) -> Ulid {
        self.send_from(Actor::Lead, kind, &Recipient::Member(role(to)), re, body)
    }

    /// A message from `sender` to `to`, which the host took (`handed_off`).
    pub fn send_from(
        &mut self,
        sender: Actor,
        kind: MessageType,
        to: &Recipient,
        re: Option<Ulid>,
        body: &str,
    ) -> Ulid {
        let now = self.clock.now();
        let new = NewMessage {
            sender,
            kind,
            re,
            commit: None,
            body: body.to_owned(),
            client_key: None,
            recipients: vec![to.clone()],
        };
        self.store
            .write(|tx| {
                let stored = tx.insert_message(now, &new)?;
                tx.set_delivery_state(
                    now,
                    &stored.message.ulid,
                    to,
                    &teamwire_store::DeliveryUpdate {
                        state: teamwire_core::DeliveryState::HandedOff,
                        route: None,
                        error: None,
                        attempts: 1,
                    },
                )?;
                Ok(stored.message.ulid)
            })
            .unwrap()
    }

    pub fn reprompt(&mut self, name: &str) {
        let now = self.clock.now();
        let role = role(name);
        self.store
            .write(|tx| tx.bump_generation(now, &role, "more work"))
            .unwrap();
    }

    pub fn remove(&mut self, name: &str) {
        let now = self.clock.now();
        let role = role(name);
        self.store
            .write(|tx| {
                tx.remove_member(now, &role, Some("removed"), &json!({}))
                    .map(|_| ())
            })
            .unwrap();
    }
}

impl Default for Bed {
    fn default() -> Self {
        Self::new()
    }
}

/// `17:24:07Z`: two digits, a colon, twice over, then `Z`.
fn is_utc_time(text: &str) -> bool {
    text.len() == 9
        && text.bytes().enumerate().all(|(i, b)| match i {
            2 | 5 => b == b':',
            8 => b == b'Z',
            _ => b.is_ascii_digit(),
        })
}

/// The events in a line, in order, each with its
/// `[teamwire:<team> HH:MM:SSZ] ` prefix taken off:
/// `["WATCHING 2 members", "TEAM-COMPLETE 2/2 done"]`. A part whose prefix
/// is missing, names another team or holds no time is returned whole, so a
/// test that compares the wording pins the prefix's shape as well.
pub fn events(line: &str) -> Vec<String> {
    line.split(" | ")
        .map(|part| {
            part.strip_prefix(&format!("[teamwire:{TEAM} "))
                .and_then(|rest| rest.split_once("] "))
                .filter(|(time, _)| is_utc_time(time))
                .map_or(part, |(_, text)| text)
                .to_owned()
        })
        .collect()
}

/// The event names in a line, in order: `LATE`, `TEAM-STALLED`, ...
pub fn names(line: &str) -> Vec<String> {
    events(line)
        .iter()
        .map(|event| event.split(' ').next().unwrap_or("").to_owned())
        .collect()
}

/// How many times each event was printed across `lines`.
pub fn counts(lines: &[String]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for line in lines {
        for name in names(line) {
            *out.entry(name).or_insert(0) += 1;
        }
    }
    out
}

pub fn count(lines: &[String], name: &str) -> usize {
    counts(lines).get(name).copied().unwrap_or(0)
}
