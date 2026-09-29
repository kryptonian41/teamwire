# Core API (`teamwire-core`, `teamwire-store` and the service crates)

This is the harness-agnostic interface every other crate builds on. The
wave-1 skeleton ticket writes the types and traits below into
`crates/teamwire-core` exactly as shown; later tickets implement them. A change
to anything in this file is a contract change and goes through the lead.

Signatures are Rust. `…` marks fields that are plain data and obvious from
the data model in `ARCHITECTURE.md`.

## 1. Names and ids

```rust
/// `^[a-z][a-z0-9_-]{0,31}$`. Checked by `TeamName::parse`.
pub struct TeamName(String);
/// Same pattern as TeamName. `"lead"` is reserved and refused as a role.
pub struct Role(String);
/// `<team>-<role>`, the member's address in hosts and harnesses.
pub struct Address(String);

pub struct Ulid(ulid::Ulid);          // identity of messages, completions, events, teams
pub struct DisplayId(String);          // "m014": shown to agents; unique per team
pub struct SessionId(String);          // the harness's own session id, opaque
pub struct Place { pub host: HostKind, pub id: String }   // e.g. herdr "w3T:p3"
pub struct Pid(u32);

pub enum HostKind { Herdr, Tmux, None, Fake }
pub enum HarnessKind { ClaudeCode, Fake }

/// Who is acting. Proven before any rule runs (ADR 0008).
pub enum Actor { Lead, Member(Role) }
```

## 2. Members

```rust
pub enum MemberKind { Implementer, Reviewer, Investigator, Closer }
pub enum Mode { Worktree, Pane }
pub enum MemberState { Creating, Started, Failed, Removed }

pub struct Member {
    pub role: Role, pub title: String, pub kind: MemberKind, pub mode: Mode,
    pub harness: HarnessKind, pub host: HostKind, pub place: Option<Place>,
    pub session_id: Option<SessionId>, pub cwd: PathBuf,
    pub repo: Option<PathBuf>, pub branch: Option<String>, pub base: Option<String>,
    pub base_sha: Option<String>, pub check: Option<String>, pub owns: Vec<String>,
    pub deadline_s: u64, pub state: MemberState, pub generation: u32,
    pub gen_started_at: Timestamp, /* … */
}
```

## 3. Messages and deliveries

```rust
pub enum MessageType { Notice, Finding, Question, Answer, Reprompt, Note }

pub struct SendRequest {
    pub team: TeamName,
    pub actor: Actor,                 // proven identity, not a claim
    pub to: Vec<Recipient>,           // Recipient::Lead | Recipient::Member(Role)
    pub kind: MessageType,
    pub body: String,
    pub commit: Option<String>,       // required for Notice
    pub re: Option<DisplayId>,        // required for Answer
    pub client_key: Option<String>,   // idempotency key, unique per sender
}

pub struct Message {
    pub ulid: Ulid, pub display_id: DisplayId, pub sender: Actor, pub seq: u64,
    pub kind: MessageType, pub re: Option<Ulid>, pub commit: Option<String>,
    pub body: String, pub body_sha256: [u8; 32], pub created_at: Timestamp,
}

pub enum DeliveryState { Pending, HandedOff, Unconfirmed, Undelivered, Acked }

pub struct Delivery {
    pub message: Ulid, pub recipient: Recipient, pub state: DeliveryState,
    pub attempts: u32, pub route: Option<Route>,
    pub error: Option<DeliveryError>, pub acked_at: Option<Timestamp>,
}

pub enum Route { HostTyping, Inbox, Watch }

pub struct DeliveryError { pub code: DeliveryErrorCode, pub detail: String }

pub enum DeliveryErrorCode {
    // from the locator (ADR 0006)
    NoLiveProcess, NoPlace, HostOtherSession, HostNoSession, HostListingFailed,
    TwoPlaces, SingleSource, BackgroundSession,
    // from the host
    HostNotReady, HostBusy, HostBlocked, HostNotFound, HostStalled, HostTimeout,
    HostBadArgument, HostOther,
    // from teamwire
    RecipientRemoved, TeamClosed, NoHost,
}

/// Why a send was refused before anything was stored (MSG-1..MSG-3, MSG-9, MSG-14, MSG-15).
pub enum Refusal {
    Identity, TypeNotAllowed, RecipientNotAllowed, UnknownRecipient,
    RemovedRecipient, SelfMessage, EmptyBody, TooLong { limit: usize, got: usize },
    NoCommit, CommitNotReachable, NotAQuestion, NotAskedOfYou, TeamClosed,
    RateLimited, Repeat,
}
```

## 4. Completions

```rust
pub enum CompletionStatus { Done, Blocked, Failed }
pub enum ReportState { Pending, Delivering, Delivered, Undelivered, Unconfirmed }
pub enum FinalSource { Hook, Transcript, None }

pub struct Completion {
    pub ulid: Ulid, pub role: Role, pub generation: u32, pub n: u32,
    pub status: CompletionStatus, pub summary: String,
    pub final_message: Option<String>, pub final_source: FinalSource,
    pub report_state: ReportState, pub route: Option<Route>,
    pub reason: Option<DeliveryError>, pub created_at: Timestamp,
}

pub struct ReportGaps {
    pub report_missing: bool,
    pub commits_without_line: Vec<String>,   // 7-char shas
    pub decisions_missing: bool,
}
```

## 5. Events

```rust
pub struct Event {
    pub seq: u64, pub ulid: Ulid, pub time: Timestamp, pub kind: EventType,
    pub role: Option<Role>, pub subject: Option<Ulid>,
    pub data: serde_json::Value, pub writer_version: String,
}
```

`EventType` is a closed enum. Its string forms are the `type` column and the
names in `docs/metrics.md` section 1:

```
team.created team.closing team.closed team.lead_adopted
member.creating member.started member.failed member.removed member.reprompted
member.completed member.completion_refused member.held
message.sent message.refused
delivery.handed_off delivery.unconfirmed delivery.undelivered delivery.acked delivery.reminded
result.delivering result.delivered result.undelivered result.unconfirmed
watch.started watch.line watch.stopped
verify.recorded verify.waived
close.step close.refused
store.error
```

## 6. Traits

All methods take a `timeout` or are bounded inside the adapter. Every error is
a typed code, never a string to match (ADP-5).

```rust
pub trait Host: Send + Sync {
    fn kind(&self) -> HostKind;
    fn capabilities(&self) -> HostCaps;
    /// Create a new place for a member and run `argv` in it.
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError>;
    /// Type `text` and submit it. `Ok` means the host accepted it, no more.
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError>;
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError>;
    /// Who occupies `place`: a session id if the host knows it, else process ids.
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError>;
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError>;
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError>;
    fn close(&self, place: &Place, timeout: Duration) -> Result<(), HostError>;
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError>;
    /// The environment variable a process uses to name its own place, if any
    /// (herdr: "HERDR_PANE_ID").
    fn place_env_var(&self) -> Option<&'static str>;
}

pub struct HostCaps { pub can_type: bool, pub reports_state: bool,
                      pub knows_session_of_place: bool, pub submit_ack: SubmitAck }
pub enum PlaceState { Idle, Working, Blocked, Gone, Unknown }
pub struct Occupant { pub session: Option<SessionId>, pub pids: Vec<Pid> }
pub struct HostError { pub code: HostErrorCode, pub detail: String }
pub enum HostErrorCode { NotReady, Busy, Blocked, NotFound, Stalled, Timeout, BadArgument, Other }

pub trait Harness: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn capabilities(&self) -> HarnessCaps;
    /// argv, env and files to start one member. Files are written by launch
    /// under the team directory before spawn; nothing outside it.
    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError>;
    /// The caller's own session, read from its environment.
    fn current_session(&self) -> Option<CallerSession>;   // id + kind (interactive | background)
    fn processes_for_session(&self, s: &SessionId, timeout: Duration) -> Result<Vec<Pid>, HarnessError>;
    fn is_alive(&self, s: &SessionId, timeout: Duration) -> Result<Liveness, HarnessError>;
    /// Parse turn-end hook stdin into the neutral form.
    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError>;
    /// Render the hook reply that holds the member once with `note`.
    fn hold_reply(&self, note: &str) -> Vec<u8>;
    /// Handle a startup dialog on the member's screen, if the harness has one.
    fn answer_startup(&self, host: &dyn Host, place: &Place, policy: TrustPolicy)
        -> Result<StartupOutcome, HarnessError>;
    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)>;
}

pub struct HarnessCaps { pub turn_end_hook: bool, pub in_turn_hook: bool,
                         pub mcp_client: bool, pub choose_session_id: bool }
pub struct TurnEnd { pub session: SessionId, pub subagent: bool,
                     pub hold_already_active: bool, pub final_message: Option<String>,
                     pub transcript: Option<PathBuf> }
pub enum Liveness { Alive(PlaceState), Dead, Unknown }
pub enum StartupOutcome { NoDialog, AnsweredPlainTrust, RefusedEscalated, RefusedHome }

pub trait Platform: Send + Sync {
    fn pid_alive(&self, pid: Pid) -> bool;
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError>;
    /// Run in a new process group; on timeout kill the group; exit 124.
    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError>;
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError>;
    fn sleep_hold(&self) -> Option<&dyn SleepHold>;
    fn free_bytes(&self, path: &Path) -> Result<u64, PlatformError>;
    fn on_network_fs(&self, path: &Path) -> Result<bool, PlatformError>;
}

pub trait SleepHold {
    fn find_existing(&self, lead_pid: Pid) -> Option<HoldInfo>;
    fn start(&self, lead_pid: Pid) -> Result<HoldInfo, PlatformError>;
    /// Kills only if `hold.pid` is still an untimed hold on `lead_pid`.
    fn release(&self, hold: &HoldInfo, lead_pid: Pid) -> Result<ReleaseOutcome, PlatformError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;         // wall clock, stored
    fn monotonic(&self) -> Instant;     // timeouts inside a process
}
```

`OneLine` is a string that has been through `sanitize::one_line(text, max)`:
control characters and newlines removed, other non-printable characters
dropped, cut to `max` characters with `…`. It is the only type `Host::submit`
accepts, so an unsanitised string cannot reach a session (DEL-11).

## 7. Store (`teamwire-store`)

```rust
pub struct Store { /* one rusqlite::Connection, never shared across processes */ }

impl Store {
    /// Creates the directory and database if missing; applies pragmas;
    /// migrates forward; refuses a newer schema (StoreError::SchemaNewer).
    pub fn open(home: &Path, team: &TeamName) -> Result<Store, StoreError>;
    pub fn open_existing(home: &Path, team: &TeamName) -> Result<Store, StoreError>;
    /// Runs `f` inside BEGIN IMMEDIATE; commits on Ok, rolls back on Err.
    pub fn write<T>(&mut self, f: impl FnOnce(&Tx) -> Result<T, StoreError>) -> Result<T, StoreError>;
    pub fn read<T>(&self, f: impl FnOnce(&ReadTx) -> Result<T, StoreError>) -> Result<T, StoreError>;
    pub fn backup_to(&self, path: &Path) -> Result<(), StoreError>;
    pub fn quick_check(&self) -> Result<String, StoreError>;
    pub fn integrity_check(&self) -> Result<String, StoreError>;
}

pub enum StoreError { Busy, Full, SchemaNewer { db: u32, binary: u32 },
                      NotTeamwire, Corrupt(String), Constraint(String), Io(String) }
```

`Tx` offers one typed method per operation the services need, for example
`insert_message`, `set_delivery_state`, `next_counter`, `insert_completion`,
`claim_completion_for_delivery`, `bump_generation`, `set_member_state`,
`record_event`, `announce_once` (returns false if the key already exists).
Every `Tx` method that changes a row also records the matching event.

Schema version 1 lives in `crates/teamwire-store/src/schema/v1.sql`. Its
tables and columns are those in `ARCHITECTURE.md`, "Data model". Constraints
that carry integrity:

```sql
CREATE UNIQUE INDEX messages_sender_key ON messages(sender, client_key);
CREATE UNIQUE INDEX messages_sender_seq ON messages(sender, seq);
CREATE UNIQUE INDEX messages_display ON messages(display_id);
CREATE UNIQUE INDEX completions_role_n ON completions(role, n);
-- deliveries: PRIMARY KEY (message_ulid, recipient), FOREIGN KEY to messages
-- members.state, deliveries.state, completions.status, completions.report_state:
--   CHECK (state IN (...)) with the enum values above
```

## 8. Services

Each service crate exposes plain functions over a `Ctx` that carries the
store, the adapters, the clock and the config. The CLI and the MCP server
call the same functions.

```rust
pub struct Ctx<'a> {
    pub home: &'a Path, pub store: &'a mut Store,
    pub host: &'a dyn Host, pub harness: &'a dyn Harness,
    pub platform: &'a dyn Platform, pub clock: &'a dyn Clock, pub config: &'a Config,
}

// teamwire-deliver
pub fn locate(ctx: &Ctx, session: &SessionId, recorded_kind: SessionKind) -> Result<Place, DeliveryError>;
pub fn deliver_line(ctx: &Ctx, session: &SessionId, line: &OneLine) -> DeliveryOutcome;
// DeliveryOutcome = HandedOff | Unconfirmed(DeliveryError) | Undelivered(DeliveryError)

// teamwire-msg
pub fn prove_actor(ctx: &Ctx, env: &CallerEnv) -> Result<Actor, Refusal>;
pub fn send(ctx: &mut Ctx, req: SendRequest) -> Result<SendReport, SendError>;
pub fn inbox(ctx: &mut Ctx, me: &Role, all: bool) -> Result<Vec<InboxItem>, SendError>;
pub fn ack(ctx: &mut Ctx, me: &Role, ids: &[DisplayId]) -> Result<Vec<AckResult>, SendError>;
pub fn log(ctx: &Ctx, filter: LogFilter) -> Result<Vec<LogRow>, SendError>;

// teamwire-report
pub fn done(ctx: &mut Ctx, me: &Role, status: CompletionStatus, summary: &str) -> Result<DoneReport, DoneError>;
pub fn report_gaps(ctx: &Ctx, member: &Member) -> ReportGaps;   // errors → empty gaps
pub fn on_turn_end(ctx: &mut Ctx, me: &Role, t: &TurnEnd) -> TurnEndReply; // Continue | Hold(note)
pub fn on_tool_use(ctx: &mut Ctx, me: &Role) -> Option<String>;            // reminder text

// teamwire-launch
pub fn validate(spec: &TeamSpec, add: bool, ctx: &Ctx) -> Result<ValidPlan, Vec<SpecError>>;
pub fn launch(ctx: &mut Ctx, plan: ValidPlan) -> LaunchReport;          // per-member outcomes

// teamwire-verify
pub fn verify(ctx: &mut Ctx, role: Option<&Role>, opts: VerifyOpts) -> Vec<VerifyRecord>;
pub fn waive(ctx: &mut Ctx, role: &Role, flag: &FlagId, reason: &str) -> Result<(), VerifyError>;

// teamwire-close
pub fn plan_close(ctx: &Ctx, target: CloseTarget) -> ClosePlan;          // for --dry-run
pub fn close(ctx: &mut Ctx, target: CloseTarget, opts: CloseOpts) -> CloseReport;

// teamwire-watch
pub fn poll(ctx: &mut Ctx, state: &mut WatchMemory) -> Option<OneLine>;  // at most one line
pub fn status(ctx: &Ctx) -> StatusView;                                  // reads only

// teamwire-otel
pub fn metrics(ctx: &Ctx, q: MetricsQuery) -> MetricsView;
pub fn export_once(ctx: &mut Ctx, exporter: &OtlpConfig) -> Result<ExportReport, ExportError>;
```
