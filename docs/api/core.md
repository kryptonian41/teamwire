# Core API (`teamwire-core`, `teamwire-store` and the service crates)

This is the harness-agnostic interface every other crate builds on. The
wave-1 skeleton ticket writes the types and traits below into
`crates/teamwire-core` exactly as shown; later tickets implement them. A change
to anything in this file is a contract change and goes through the lead.

Signatures are Rust. `…` marks fields that are plain data and obvious from
the data model in `ARCHITECTURE.md`.

## 1. Names and ids

```rust
/// `^[a-z][a-z0-9]{0,11}$`: no hyphen, no underscore. Checked by `TeamName::parse`.
pub struct TeamName(String);
/// `^[a-z][a-z0-9_]{0,15}$`: no hyphen. `"lead"` is reserved and refused.
/// `"user"` parses, so stored rows still read, but launch refuses a spec
/// role named `user` with `reserved_role` (ADR 0040).
pub struct Role(String);
/// `tw_<team>_<role>`, at most 32 characters: the member's name in hosts and
/// harnesses. It never contains a hyphen, so it never equals a `<team>-<role>`
/// name that another tool gives its agents (INV-32).
pub struct Address(String);

pub struct Ulid(ulid::Ulid);          // identity of messages, completions, events, teams
pub struct DisplayId(String);          // "m014": shown to agents; unique per team
pub struct SessionId(String);          // the harness's own session id, opaque
pub struct Place { pub host: HostId, pub id: String }     // e.g. herdr "w3T:p3"
pub struct Pid(u32);

/// Adapter ids are open strings, `^[a-z][a-z0-9-]{0,31}$`: "claude-code",
/// "herdr", "tmux", "none", "fake-a". The core never matches on a value; it
/// looks the id up in the `Registry` (§6a) and reads the adapter's
/// capability record. Adding an adapter crate adds a registry entry, not a
/// core type.
pub struct HarnessId(String);
pub struct HostId(String);

/// Who is acting. Proven before any rule runs (ADR 0008). `User` is the
/// human user, proven by the user key, string form `user` (ADR 0040).
/// `Other` is a sender in another team, `role@team` or `lead@team`; it
/// appears only on a stored message, never from a proof (ADR 0016).
pub enum Actor { Lead, User, Member(Role), Other(Remote) }

/// An agent of another team: `lead@<team>` or `<role>@<team>`.
pub struct Remote { /* team, role (None for the lead) */ }
```

## 2. Members

```rust
pub enum MemberKind { Implementer, Reviewer, Investigator, Closer, PlanReviewer, Reconciler }
impl MemberKind {
    pub fn is_quiet(self) -> bool;                       // the kind alone: reviewers, plan reviewers, reconcilers and closers (MSG-3)
    pub fn may_send(self, kind: MessageType) -> bool;    // a member never sends reprompt or note; a quiet kind sends nothing
}
pub enum Mode { Worktree, Pane }
pub enum MemberState { Creating, Started, Failed, Removed }

pub struct Member {
    pub role: Role, pub title: String, pub kind: MemberKind, pub mode: Mode,
    pub harness: HarnessId, pub host: HostId, pub place: Option<Place>,
    pub session_id: Option<SessionId>, pub cwd: PathBuf,
    pub repo: Option<PathBuf>, pub branch: Option<String>, pub base: Option<String>,
    pub base_sha: Option<String>, pub check: Option<String>, pub owns: Vec<String>,
    pub deadline_s: u64, pub state: MemberState, pub generation: u32,
    pub gen_started_at: Timestamp, /* … */
    pub quiet: bool,                  // what the spec's `quiet` said; false for a quiet kind with no key
    pub set: Option<String>,          // the spec's set label
}
impl Member {
    pub fn is_quiet(&self) -> bool;                      // its kind is quiet, or `quiet` is true (MSG-3)
    pub fn may_send(&self, kind: MessageType) -> bool;   // what its kind may send, and nothing when quiet
}
```

Every rule that decides silence asks `Member::is_quiet` or
`Member::may_send`, never the kind alone: the sender and recipient checks of
a send, the check of a recipient in another team, and the roster. A
`Member`'s JSON (`teamwire export`) writes `quiet` in effect, the answer of
`is_quiet`, so a reviewer's JSON says `"quiet": true` although its stored
value is false. A reconciler reads the reports of one set of readers and
writes the one report the next step acts on; it runs in `pane` mode only
(ADR 0057). A set is the members with the same `set` label in one team.

## 3. Messages and deliveries

```rust
pub enum MessageType { Notice, Finding, Question, Answer, Reprompt, Note }

/// The first words of `teamwire amend`'s note. `close --role` finds the
/// newest such note from `lead` to a member and counts no verify record made
/// at or before it (CLS-9, ADR 0046).
pub const AMEND_MARKER: &str = "CHECK AMENDED:";

pub struct SendRequest {
    pub team: TeamName,
    pub actor: Actor,                 // proven identity, not a claim
    pub to: Vec<Recipient>,           // Lead | Member(Role) | Other(Remote)
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

/// The mechanism that carried an item, not a product name.
/// `HarnessPush` is a harness that puts text into its own session
/// (opencode `prompt_async`, Claude Channels); no v1 adapter uses it.
pub enum Route { HostTyping, HarnessPush, Inbox, Watch }

pub struct DeliveryError { pub code: DeliveryErrorCode, pub detail: String }

pub enum DeliveryErrorCode {
    // from the locator (ADR 0006)
    NoLiveProcess, NoPlace, HostOtherSession, HostNoSession, HostListingFailed,
    TwoPlaces, SingleSource, BackgroundSession, SessionBlocked, NotAtInputPrompt,
    // from the registry: a recorded adapter id this binary does not have
    UnknownAdapter,
    // from the host
    HostNotReady, HostBusy, HostBlocked, HostNotFound, HostStalled, HostTimeout,
    HostBadArgument, HostOther,
    // from teamwire
    RecipientRemoved, TeamClosed, NoHost,
    DeliveryInterrupted,  // left in `delivering` past `delivering_stale_s` (REP-14)
    BudgetExhausted,      // the turn-end hook's delivery budget ran out (ADR 0007)
    HeldForHandoff,       // the recipient asked for a handoff; nothing was typed (ADR 0043)
}

/// Why a send was refused before anything was stored (MSG-1..MSG-3, MSG-9, MSG-14, MSG-15, MSG-20).
pub enum Refusal {
    Identity, TypeNotAllowed, RecipientNotAllowed, UnknownRecipient,
    UserLeadWithMember,  // code recipient_not_allowed: the user named lead with a member (ADR 0040)
    UnknownTeam, RecipientTeamClosed,  // role@team names no team, or a team not open
    RemovedRecipient, SelfMessage, EmptyBody, TooLong { limit: usize, got: usize },
    NoCommit, CommitNotReachable, NotAQuestion, NotAskedOfYou, TeamClosed,
    RateLimited, Repeat,
    NotResendable,  // only the sender or the lead may resend, to a resendable recipient
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
team.created team.closing team.closed team.lead_adopted team.refs_added
member.creating member.started member.failed member.refused member.replaced member.removed member.reprompted member.woken member.handoff
member.completed member.completion_refused member.held
dialog.opened dialog.answered dialog.closed
message.sent message.refused
delivery.handed_off delivery.unconfirmed delivery.undelivered delivery.acked delivery.reminded delivery.resent delivery.reentered delivery.recipient_stopped
result.delivering result.delivered result.undelivered result.unconfirmed
watch.started watch.line watch.stopped
verify.recorded verify.waived
close.step close.refused
store.error
```

Some `data` keys are read back by `teamwire reopen` or `teamwire metrics`. They live in event data,
so they need no schema change:

| Event | Key | Meaning |
| --- | --- | --- |
| `member.started` | `model`, `effort` | What the member was launched with. `null` when the spec named none. A missing key means an older build started the member. |
| `member.started` | `excluded` | The `info/exclude` lines this launch added, as `[{ "file": <absolute path>, "lines": [...] }]`, one entry per append (setup files, then `/TASK.md`). A line that was already in the file is not listed. Close takes these back. A missing key means an older build started the member. |
| `result.delivering` | `usage`, `session` | `usage` maps a model name to the session's running totals `input`, `output`, `cache_read` and `cache_creation`, counted once per `message.id` across the member's main transcript and its subagent transcripts (`<session-id>/subagents/*.jsonl`). A subagent file that would take the sum past 64 MiB, is not a regular file or cannot be read is skipped, and the rest still count. `session` is the session id, so a new session's totals are not taken as a continuation. `usage` is missing when the transcript is missing, unreadable, over 64 MiB, not a regular file, or holds no assistant usage; `session` is always written. `teamwire metrics` reads them. |
| `team.created` | `lead_cwd` | The folder of the lead's session at launch, when the harness can name it. |
| `team.created` | `refs` | The spec's ticket refs as `{"provider", "id"}` objects, deduplicated, in order; `[]` for none. A missing key means an older build created the team. |
| `team.refs_added` | `added` | The refs, as objects, that a later `launch --add` appended to the team, in order (ADR 0038). |
| `team.created` | `afk_budget_ends_at` | When an AFK run's budget ends, written by `teamwire afk start`. Missing on an ordinary team. |
| `team.lead_adopted` | `lead_cwd` | The same, for the session that took over. |

`ReadTx::lead_cwd` returns the `lead_cwd` of the newest `team.created` or
`team.lead_adopted` event, even when that event holds none, because an older
folder belongs to an older lead session. `ReadTx::run_options` returns a
member's recorded `model` and `effort`, and whether either key was recorded.
`Tx::create_team_with` and `Tx::adopt_lead_in` write the extra keys.
`Tx::create_afk_team` creates an AFK run's team: `create_team_with`, with
its extra keys, plus `team.afk_budget_ends_at` and the event key of the same
name.

## 6. Traits

All methods take a `timeout` or are bounded inside the adapter. Every error is
a typed code, never a string to match (ADP-5).

```rust
pub trait Host: Send + Sync {
    fn id(&self) -> HostId;
    fn capabilities(&self) -> HostCaps;
    /// Create a new place for a member and run `argv` in it. A name a live
    /// agent holds is refused with `BadArgument` and a detail starting
    /// `agent_name_taken:` (herdr, tmux and the fake); on tmux a pane whose
    /// program has exited holds no live agent. A method that runs several host
    /// commands splits the caller's timeout across them, so their sum never
    /// passes it (tmux `submit`, `send_keys`, `close`, `spawn`; herdr `spawn`
    /// uses a deadline).
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError>;
    /// Type `text` and submit it. `Ok` means the host accepted it, no more.
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError>;
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError>;
    /// Who occupies `place`: a session id if the host knows it, else process ids.
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError>;
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError>;
    /// `read_tail` with the SGR style codes kept (herdr `--format ansi`, tmux
    /// `capture-pane -e`). Default: the plain tail.
    fn read_tail_styled(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError>;
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError>;
    /// Close `place` only if the agent in it is `expected`. A place that holds
    /// another agent, or none, is left alone and reported `NotFound`: a place
    /// id can be reused after the member's agent died (CLS-7).
    fn close(&self, place: &Place, expected: &Address, timeout: Duration) -> Result<(), HostError>;
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError>;
    /// Names of every live agent the host knows, from any tool (for LCH-25).
    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError>;
    /// The environment variable a process uses to name its own place, if any
    /// (herdr: "HERDR_PANE_ID").
    fn place_env_var(&self) -> Option<&'static str>;
    /// The place id one process names for itself, read from its environment.
    /// Default: the value of `place_env_var`. tmux joins `TMUX_PANE` with the
    /// server named by `TMUX`, as `%<n>@<socket path>`.
    fn place_from_env(&self, read: &dyn Fn(&str) -> Option<String>) -> Option<String>;
    /// Split `beside` and run `req.argv` in the new pane, which gets
    /// `req.label` and holds no teamwire agent name (ADR 0051). herdr runs
    /// `pane split --direction right --no-focus`, `pane rename`, then
    /// `pane run` of `sh '<folder>/run'`, a private run file that removes its
    /// folder and `exec`s the command, because herdr refuses a `pane run`
    /// line of 1,024 bytes or more on macOS. A failure after
    /// the split closes the new pane. tmux runs `split-window` with the
    /// environment through the private env file (ADR 0013) and sets the pane
    /// title. Default `Unsupported`.
    fn split_run(&self, beside: &Place, req: &SplitRequest, timeout: Duration) -> Result<Place, HostError>;
    /// Close a pane that holds no teamwire agent name. The method checks
    /// nothing about the occupant. The caller may close only a pane it opened
    /// with `split_run` in the same command, or a pane whose occupant it has
    /// proven with the locator right before this call. A pane that is already
    /// gone counts as closed. herdr `pane close`, tmux `kill-pane`. Default
    /// `Unsupported`.
    fn close_pane(&self, place: &Place, timeout: Duration) -> Result<(), HostError>;
}

pub struct HostCaps { pub can_type: bool, pub reports_state: bool,
                      pub knows_session_of_place: bool, pub submit_ack: SubmitAck,
                      /// The host can give the started program environment
                      /// variables (tmux, through a private file, ADR 0013). herdr cannot.
                      pub sets_env_at_spawn: bool,
                      /// The host itself refuses to type into a session that
                      /// shows a dialog (herdr `agent_blocked`). tmux does not.
                      pub guards_dialogs: bool,
                      /// The host offers `split_run` and `close_pane` (herdr,
                      /// tmux). `NoHost` does not.
                      pub splits_pane: bool }
pub enum PlaceState { Idle, Working, Blocked, Gone, Unknown }
pub struct Occupant { pub session: Option<SessionId>, pub pids: Vec<Pid> }
pub struct HostError { pub code: HostErrorCode, pub detail: String }
pub enum HostErrorCode { NotReady, Busy, Blocked, NotFound, Stalled, Timeout, BadArgument, Unsupported, Other }

pub trait Harness: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> HarnessCaps;
    /// argv, env and files to start one member. Files are written by launch
    /// under the team directory before spawn; nothing outside it.
    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError>;
    /// argv to resume an existing member session with the same per-run
    /// configuration (shown by `status --role`, run by `reopen`). Build the
    /// `MemberLaunch` of a member that already ran with `MemberLaunch::for_resume`.
    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError>;
    /// The caller's own session (id + kind, interactive | background), read
    /// through `get`: the process environment (`|name| std::env::var(name).ok()`).
    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession>;
    /// argv that resumes the lead's own session: a plain resume with no per-run
    /// files, because the lead loads the teamwire plugin itself. Default `Unsupported`.
    fn lead_resume_argv(&self, s: &SessionId) -> Result<Vec<String>, HarnessError>;
    /// The command line that starts a fresh interactive lead session like
    /// `req.old_session`, with `req.prompt` as its first prompt (ADR 0051).
    /// The harness carries the old lead's flags by its own `CarryRules` and
    /// names each flag it drops. Claude Code: `claude --session-id <new>
    /// <carried> <prompt>`, where the old transcript's newest main-chain model
    /// wins over the old flags, and so does its newest permission mode. A
    /// newest mode `claude --permission-mode` does not accept passes no
    /// permission flag at all, drops each old one, and gives a warning. pi:
    /// `pi --session-id <new> <carried> -- <prompt>`. Claude Code and pi
    /// refuse a missing id. Default `Unsupported`.
    fn lead_start(&self, req: &LeadStart) -> Result<LeadStartSpec, HarnessError>;
    /// The lead setup `install`, `uninstall` and `doctor` reach this harness's
    /// lead through (ADR 0031). Default `None`, for a harness that cannot lead.
    fn lead_setup(&self) -> Option<&dyn LeadSetup>;
    /// Why the caller's session cannot lead a new team, when the harness can
    /// tell its lead setup is not loaded there (pi without the lead package,
    /// ADR 0030). Launch refuses with `lead_setup_missing`. Default `None`.
    fn lead_setup_missing(&self) -> Option<String>;
    /// The folder a session runs in, read without writing: the live session
    /// first, then the session's own record. `None` when neither names it.
    /// Default `Ok(None)`.
    fn session_cwd(&self, s: &SessionId) -> Result<Option<PathBuf>, HarnessError>;
    /// How full the session's context window is, read without writing.
    /// `transcript` is the path a hook was handed; without it the harness
    /// finds the session's own transcript. `Ok(None)` means unknown (none yet,
    /// or a compaction left none), never zero. Default `Ok(None)`.
    fn context_fill(&self, s: &SessionId, transcript: Option<&Path>) -> Result<Option<Fill>, HarnessError>;
    /// The tool call the session runs now: the newest call with no result,
    /// read without writing from the harness's own session record. `cwd` is
    /// the session's working directory. `Ok(None)` means none runs, or the
    /// harness cannot tell. Default `Ok(None)`. Claude Code reads the
    /// transcript's newest `tool_use` with no `tool_result`, and
    /// pi the `tool` field its member extension writes to the state file.
    fn current_tool(&self, s: &SessionId, cwd: Option<&Path>) -> Result<Option<ToolRun>, HarnessError>;
    /// The session's token usage so far, per model, read without writing from
    /// the harness's own session file. `transcript` as for `context_fill`.
    /// `Ok(None)` means no reading. Default `Ok(None)`: the turn-end then
    /// reads the hook's transcript with the Claude Code reader in
    /// teamwire-report. pi sums every assistant message's `input`, `output`,
    /// `cacheRead` and `cacheWrite`.
    fn session_usage(&self, s: &SessionId, transcript: Option<&Path>) -> Result<Option<SessionUsage>, HarnessError>;
    fn processes_for_session(&self, s: &SessionId, timeout: Duration) -> Result<Vec<Pid>, HarnessError>;
    fn is_alive(&self, s: &SessionId, timeout: Duration) -> Result<Liveness, HarnessError>;
    /// Parse turn-end hook stdin into the neutral form.
    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError>;
    /// Render the hook reply that holds the member once with `note`.
    fn hold_reply(&self, note: &str) -> Vec<u8>;
    /// Parse the lead's tool hook stdin into the neutral form (ADR 0035).
    /// Default `Unsupported`, and the hook then says nothing. Claude Code
    /// parses its `PostToolUse` input; pi parses the input teamwire's
    /// lead extension writes.
    fn parse_tool_call(&self, stdin: &[u8]) -> Result<ToolCall, HarnessError>;
    /// What kind of dialog the screen `tail` shows (ADR 0052). Default
    /// `CannotTell`, for a harness that cannot read its own dialogs. Claude
    /// Code and pi name the kind from their screen markers.
    fn dialog_kind(&self, tail: &str) -> DialogKind;
    /// Parse a permission hook's stdin into the neutral request (ADR 0052).
    /// Default `Unsupported`: the harness has no permission hook, and
    /// `dialog answer` refuses its members. Only Claude Code has one.
    fn parse_permission_request(&self, stdin: &[u8]) -> Result<PermissionRequest, HarnessError>;
    /// The bytes the permission hook prints for `decision`: allow this one
    /// call, or deny it with the decision's message. It never writes a rule,
    /// changes the input or stops the turn. Default `Unsupported`.
    fn permission_reply(&self, decision: &PermissionDecision) -> Result<Vec<u8>, HarnessError>;
    /// Classify a screen tail for delivery: at the input prompt with an empty
    /// box, showing a dialog, or unknown. A box holding text is `Unknown`, so
    /// delivery waits and then refuses with `not_at_input_prompt`. Text drawn
    /// dim (a placeholder or a prompt suggestion) does not count.
    fn screen_state(&self, tail: &str) -> ScreenState;
    /// Whether a line just submitted still sits unsent in the input box.
    /// Default false, so a harness that cannot tell gets no second Enter.
    fn line_pending(&self, tail: &str, line: &OneLine) -> bool;
    /// Whether the input box shown by `tail` holds text. Delivery waits while
    /// it does, on every host. Default false.
    fn box_holds_text(&self, tail: &str) -> bool;
    /// Handle a startup dialog on the member's screen, if the harness has one.
    fn answer_startup(&self, host: &dyn Host, place: &Place, policy: TrustPolicy)
        -> Result<StartupOutcome, HarnessError>;
    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)>;
    /// Put one line into a session without a host. Only called when
    /// `capabilities().can_push`; the default returns `Unsupported`.
    fn push(&self, s: &SessionId, line: &OneLine, timeout: Duration) -> Result<(), HostError> {
        Err(HostError::unsupported())
    }
    /// The family of `model`, a spec's `model` value for this harness, to
    /// label it and to warn about a set (ADR 0056). Default: the core's
    /// `model_family(model)`, which reads the string alone. Claude Code
    /// returns `Claude` for every value, aliases such as `opus` included,
    /// because it starts Claude models only. pi keeps the default.
    fn model_family(&self, model: &str) -> ModelFamily {
        model_family(model)
    }
}

pub struct HarnessCaps { pub turn_end_hook: bool, pub in_turn_hook: bool,
                         pub mcp_client: bool,
                         pub can_push: bool,
                         /// Environment for the session can be set in a per-run
                         /// config file (Claude Code `settings.json` `env`).
                         pub session_env_via_config: bool,
                         /// How a lead starts `teamwire watch`; every text that tells a
                         /// lead to start one (Stop hold, result line, reprompt, launch)
                         /// is worded from it by `teamwire_deliver::watch_text`
                         pub watch_start: WatchStart,
                         /// A lead on this harness can start an AFK run (AFK-2, ADR 0031)
                         pub runs_afk: bool,
                         /// The full sentence that tells a lead on this harness to load
                         /// the teamwire skill, from the harness's lead file; empty for a
                         /// harness that cannot lead. The first prompt of a lead handoff
                         /// uses it (ADR 0051). Never read back from JSON
                         pub load_skill: &'static str }
/// MonitoredCommand: the lead runs the command under a tool that hands it each
/// line (Claude Code's Monitor). ExtensionTool: the lead calls teamwire's lead
/// extension tool `teamwire_watch` (pi, ADR 0036).
pub enum WatchStart { MonitoredCommand, ExtensionTool }
pub struct TurnEnd { pub session: SessionId, pub subagent: bool,
                     pub hold_already_active: bool, pub final_message: Option<String>,
                     // read only when it is a regular file of at most 64 MiB,
                     // for the final-message fallback and the usage alike
                     pub transcript: Option<PathBuf> }
/// One tool call of the lead's session (ADR 0035). `cwd` is the folder a
/// relative path in `action` is read against.
pub struct ToolCall { pub session: SessionId, pub subagent: bool,
                      pub cwd: Option<PathBuf>, pub action: ToolAction }
pub enum ToolAction { Edit(Vec<PathBuf>), Shell(String), Other }
pub enum Liveness { Alive(PlaceState), Dead, Unknown }
pub enum ScreenState { InputPrompt, Dialog, Unknown }
pub enum StartupOutcome { NoDialog, AnsweredPlainTrust, RefusedEscalated }

/// A process: its pid, and when it started if the platform could read it.
/// `From<Pid>` gives one with the start time unknown.
pub struct ProcessRef { pub pid: Pid, pub started: Option<Timestamp> }
/// How far two readings of one process's start time may differ.
pub const START_TIME_SLACK: Duration = Duration::from_secs(2);

pub trait Platform: Send + Sync {
    fn pid_alive(&self, pid: Pid) -> bool;
    /// When the process with `pid` started, to the second; None when
    /// unknown. Default None, so the pid alone decides.
    fn process_started(&self, pid: Pid) -> Option<Timestamp> { None }
    /// The pid is alive and, when both start times are known, started within
    /// START_TIME_SLACK of the recorded time. A reused pid is not the process.
    fn process_alive(&self, process: ProcessRef) -> bool { /* default: pid_alive, then compare start times when both are known */ }
    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError>;
    /// The arguments the process `pid` was started with, `argv[0]` first,
    /// each as the process got it (the kernel's own list, so a value with a
    /// space comes back whole). None when no process has the pid, or this
    /// platform cannot tell. Default None.
    fn args_of(&self, pid: Pid) -> Option<Vec<String>> { None }
    /// Run in a new process group; on timeout kill the group; exit 124.
    /// Children start one at a time, so no child inherits another's pipes.
    /// Output is kept as it is read: if a process the child left behind
    /// holds a pipe open 2 s after the child ends, what was read is returned.
    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError>;
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError>;
    /// Create a directory and its missing parents; delete a directory and all
    /// in it (a symlink loses only the link). Both default to `Unsupported`,
    /// for a platform that writes no files; the lead setup's `apply` uses them.
    fn create_dir_all(&self, path: &Path) -> Result<(), PlatformError>;
    fn remove_dir_all(&self, path: &Path) -> Result<(), PlatformError>;
    /// The kernel's exclusive `flock` on the file `path`. The file and its
    /// folder are made when missing, and the file is never written. A free
    /// lock is taken at once. A busy one is tried every 50 ms until `wait`
    /// runs out, then the call fails with `PlatformErrorCode::TimedOut`
    /// ("timed_out"). A `wait` too large for the clock has no limit. Dropping
    /// the guard releases the lock, and so does the holder's death. Default
    /// `Unsupported`. Launch's and close's exclude, worktree and user-key
    /// locks all go through it.
    fn lock_file(&self, path: &Path, wait: Duration) -> Result<FileLock, PlatformError>;
    fn sleep_hold(&self) -> Option<&dyn SleepHold>;
    fn free_bytes(&self, path: &Path) -> Result<u64, PlatformError>;
    fn on_network_fs(&self, path: &Path) -> Result<bool, PlatformError>;
}

impl RunOutput {
    pub fn failure_text(&self) -> String;    // the one-line reason a failed command gives
    pub fn stdout_trimmed(&self) -> String;
}

/// The guard `lock_file` returns. Core does no I/O, so the guard is a box
/// that only the platform fills: an open file, for the real platform.
pub struct FileLock { /* private */ }
impl FileLock {
    pub fn new(held: impl Send + 'static) -> Self;   // releases the lock when `held` drops
}

pub trait SleepHold {
    fn find_existing(&self, lead_pid: Pid) -> Option<HoldInfo>;
    /// On return the hold is no descendant of the caller, so ending the
    /// caller's process tree leaves it running.
    fn start(&self, lead_pid: Pid) -> Result<HoldInfo, PlatformError>;
    /// Kills only if `hold.pid` is still an untimed hold on `lead_pid`.
    fn release(&self, hold: &HoldInfo, lead_pid: Pid) -> Result<ReleaseOutcome, PlatformError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;         // wall clock, stored
    fn monotonic(&self) -> Instant;     // timeouts inside a process
}
```

The lead setup (`teamwire_core::lead_setup`, ADR 0031). Each adapter keeps its
own mechanism and its own typed change list; the CLI sees only `LeadChange`,
which it prints, and passes back to the same harness's `apply`.

```rust
pub const LEAD_SETUP_TIMEOUT: Duration = Duration::from_secs(30);  // one command of the harness's program

/// `<bin> [--home DIR]`; each adapter renders its own entry from it.
pub struct LeadCommand { pub bin: PathBuf, pub home: Option<PathBuf> }
impl LeadCommand {
    pub fn home_text(&self) -> Option<String>;   // the home rebuilt from its components: `/x/` and `/x` agree
    pub fn words(&self) -> Vec<String>;          // bin, then `--home DIR` when given
}
pub struct LeadTarget<'a> { pub command: &'a LeadCommand,
                            pub home: &'a Path,           // the state directory
                            pub shared_skill: &'a str,    // passed in by the CLI (ADR 0023 point 9)
                            pub compare_files: bool }     // false for a cargo build under target/
pub enum LeadStatus { Missing, Stale(Vec<String>), Current }
pub enum LeadStep { WriteFiles { dir: PathBuf, files: Vec<(PathBuf, String)> },  // through the platform
                    RemoveDir(PathBuf), Run(Command),
                    Own(PathBuf) }                       // the adapter's own step (Claude's ADR 0012 skill removal)
pub struct LeadChange { /* text, step */ }
impl LeadChange { pub fn new(text: impl Into<String>, step: LeadStep) -> Self;
                  pub fn step(&self) -> &LeadStep; }
impl Display for LeadChange { /* the text, after "would" or "did" */ }
pub struct LeadPlan { pub changes: Vec<LeadChange>, pub note: Option<&'static str> }  // note: printed after apply
pub enum LeadSetupError { Read { path: PathBuf, detail: String }, Refused(String) }
impl LeadSetupError { pub fn needs_force(what: &str, verb: &str) -> Self; }  // "<what>; pass --force to <verb> it"
pub struct ApplyError<C = LeadChange> { pub done: Vec<C>, pub failed: C, pub detail: String }  // "<failed> failed: <detail>"
pub enum CheckOutcome { Pass(String), Warn(String), Fail(String) }
pub type DoctorCheck = (&'static str, CheckOutcome);
pub type Probe<'a> = &'a dyn Fn(&[&str]) -> Result<String, String>;  // runs the program through doctor's platform

pub trait LeadSetup: Send + Sync {
    fn program(&self) -> &str;            // looked for on PATH by install, uninstall, launch and doctor
    fn label(&self) -> &'static str;      // "Claude Code", "pi"
    /// Doctor fails without its program; install without a terminal or
    /// --harness installs it; uninstall always plans it and stops at its
    /// refusal. Every other harness is optional. Default false.
    fn default_lead(&self) -> bool;
    fn status(&self, target: &LeadTarget<'_>) -> Result<LeadStatus, LeadSetupError>;   // reads only
    fn plan_install(&self, target: &LeadTarget<'_>, force: bool) -> Result<LeadPlan, LeadSetupError>;
    fn plan_uninstall(&self, home: &Path, force: bool) -> Result<LeadPlan, LeadSetupError>;
    /// Makes changes this harness planned, in order; stops at the first failure.
    fn apply(&self, changes: &[LeadChange]) -> Result<(), Box<ApplyError>>;
    /// `target` is Err when the command cannot be worked out; `dir` is doctor's folder.
    fn doctor_checks(&self, target: Result<&LeadTarget<'_>, &str>, dir: &Path) -> Vec<DoctorCheck>;
    /// Checks of the program beyond its version. Default none.
    fn program_checks(&self, probe: Probe<'_>) -> Vec<DoctorCheck>;
}

/// The check of an optional harness: missing passes and names
/// `teamwire install --harness <id>`, stale and unreadable warn.
pub fn optional_check(status: Result<LeadStatus, LeadSetupError>, harness_id: &str, label: &str) -> CheckOutcome;
/// The one apply loop: makes each change with `one`, stops at the first failure.
pub fn apply_in_order<C: Clone>(changes: &[C], one: impl FnMut(&C) -> Result<(), String>)
    -> Result<(), Box<ApplyError<C>>>;
/// Makes a shared step through the platform; refuses `Own`.
pub fn make_step(platform: &dyn Platform, step: &LeadStep) -> Result<(), String>;
/// Runs the harness's program under LEAD_SETUP_TIMEOUT: stdout, or what went wrong.
pub fn run_program(platform: &dyn Platform, cmd: &Command) -> Result<String, String>;
```

`OneLine` is a string that has been through `sanitize::one_line(text, max)`:
every run of white space (line breaks and tabs included) turned into one
space and the ends trimmed, ANSI escape sequences and other control characters
removed, characters that print nothing dropped (the whole Unicode
default-ignorable set plus U+FFF9-FFFB), cut to `max` characters with
`…`. So `"a\nb"` becomes `"a b"`. It is the only type `Host::submit`
accepts, so an unsanitised string cannot reach a session (DEL-11).

## 6a. Adapter registry

```rust
/// Filled by `teamwire-cli`'s wiring with every adapter this binary has.
/// Adapter crates never register themselves into core types.
pub struct Registry { /* HarnessId -> Arc<dyn Harness>, HostId -> Arc<dyn Host> */ }

impl Registry {
    pub fn new() -> Registry;
    /// Refuses a second adapter with the same id.
    pub fn add_harness(&mut self, h: Arc<dyn Harness>) -> Result<(), RegistryError>;
    pub fn add_host(&mut self, h: Arc<dyn Host>) -> Result<(), RegistryError>;
    pub fn harness(&self, id: &HarnessId) -> Result<&dyn Harness, RegistryError>;
    pub fn host(&self, id: &HostId) -> Result<&dyn Host, RegistryError>;
    pub fn harness_ids(&self) -> Vec<&str>;   // sorted
    pub fn host_ids(&self) -> Vec<&str>;      // sorted
    /// Every harness with a lead setup, sorted by id (ADR 0031).
    pub fn lead_setups(&self) -> Vec<(&str, &dyn LeadSetup)>;
}

pub enum RegistryError { Duplicate(String), UnknownHarness(HarnessId), UnknownHost(HostId) }

/// The host for "no multiplexer" (id "none"). Every capability false; every
/// writing call returns HostError { code: NotFound, .. } and does nothing.
pub struct NoHost;
impl Host for NoHost { /* … */ }
```

The harness and host belong to each member and to the lead, never to the team
or to a service call. The team row records `lead_harness` and `lead_host`;
each member row records its own `harness` and `host`. Every service looks the
adapter up for the party it is dealing with: the lead's harness to prove the
lead or locate the lead, the recipient member's harness and host to deliver to
that member. A Claude Code lead with a member whose harness id is `pi`
type-checks and routes per member, because both harnesses (`claude-code` and
`pi`) and both hosts (`herdr` and `tmux`) are registered. A member whose
harness or host id no adapter registers has its deliveries `undelivered` with
`unknown_adapter`, and its launch is refused.

## 6a2. Permission dialogs (`teamwire_core::dialog`, ADR 0052)

The types a harness's permission hook and the lead's `dialog` commands share.
Nothing here names a lasting grant or a key.

```rust
pub enum DialogKind { NoDialog /* "none" */, Permission, Trust, Login, Update, Unknown,
                      CannotTell /* "cannot_tell": the harness cannot read its dialogs */ }
pub enum DialogVerdict { Approve, Deny }
/// Read from the events, never stored as a column.
pub enum DialogState { Pending, Answered, Taken, Expired, Unanswerable }

pub struct PermissionRequest { pub session: SessionId, pub subagent: bool, pub tool: String,
                               pub summary: String,                    // at most SUMMARY_MAX chars
                               pub input: Option<serde_json::Value>,   // None over PERMISSION_INPUT_MAX
                               pub why_not: Option<String> }           // None: the lead may answer
impl PermissionRequest { pub fn answerable(&self) -> bool; }
pub struct PermissionAnswer { pub verdict: DialogVerdict, pub reason: String }
pub struct PermissionDecision { pub verdict: DialogVerdict,
                                pub message: Option<String> }          // a denial's whole text

/// A denial's message, word for word:
///   DIALOG DENIED: the lead refused this <tool> call (dialog <id>):
///   <summary>
///   Reason: <reason>
///   Do not run this command another way, such as in a script, another shell or
///   another tool. Change your approach, or ask your lead.
pub fn decision_for(id: &Ulid, request: &PermissionRequest, answer: &PermissionAnswer)
    -> PermissionDecision;
/// Every character `one_line` drops, written as `\u{XXXX}`: control characters
/// except newline and tab, and Unicode's Default_Ignorable set (bidi controls,
/// zero-width characters). Every text a lead reads about a call goes through it.
pub fn shown(text: &str) -> String;
/// `at` as the UTC time of day, `HH:MM:SSZ`: the "answer by" time of a watch line.
pub fn clock_time(at: Timestamp) -> String;

pub const PERMISSION_WAIT: Duration = 100 s;        // the hook's wait; Claude Code denies at 120 s
pub const PERMISSION_HOOK_TIMEOUT_S: u64 = 110;     // the hook's settings timeout
pub const ANSWER_TAKE_WAIT: Duration = 5 s;         // dialog answer waits this long for "taken"
pub const PERMISSION_INPUT_MAX: usize = 65_536;     // JSON bytes the lead may answer
pub const SUMMARY_MAX: usize = 2_000;
pub const DENIED_MARKER: &str = "DIALOG DENIED:";
```

## 6b. Supporting types

The shapes T01 must write, so that T06 (which produces `LaunchSpec`) and T08
(which consumes it) agree in wave 3. Field lists are complete; types not
shown are defined above.

```rust
pub enum Recipient { Lead, Member(Role), Other(Remote) }   // no User: nobody sends to the user
pub enum SessionKind { Interactive, Background }
pub struct CallerSession { pub id: SessionId, pub kind: SessionKind }
pub enum SubmitAck { None, Submitted, StateChange }   // what Ok(()) from submit proves
pub enum TrustPolicy { Accept, Refuse }               // from the spec's accept_trust

pub struct TeamSpec { pub team: TeamName, pub title: String, pub setup: Option<String>,
                      pub harness: Option<HarnessId>, pub host: Option<HostId>,
                      pub children: Vec<MemberSpec>,
                      pub context_handoff_tokens: Option<u64>,
                      pub plugins: BTreeMap<String, bool>,      // plugin id -> on/off, all members
                      pub refs: Vec<TicketRef> }                // ticket refs, ADR 0038
pub struct TicketRef { pub provider: String, pub id: String }   // both required; no default provider
pub struct MemberSpec { pub role: Role, pub title: String, pub kind: Option<MemberKind>,
                        pub mode: Mode, pub harness: Option<HarnessId>, pub repo: Option<PathBuf>,
                        pub branch: Option<String>, pub base: Option<String>, pub cwd: Option<PathBuf>,
                        pub model: Option<String>, pub effort: Option<String>,
                        pub check: Option<String>, pub owns: Vec<String>,
                        pub task: Option<String>, pub task_file: Option<PathBuf>,
                        pub deadline_s: Option<u64>, pub setup: Option<String>,
                        pub dispatch: bool, pub accept_trust: bool,
                        pub context_handoff_tokens: Option<u64>,   // overrides the team's; 0 is refused
                        pub plugins: BTreeMap<String, bool>,    // overrides the team's, per key
                        pub quiet: Option<bool>,                // true makes any kind quiet; false on a quiet kind is refused (ADR 0058)
                        pub set: Option<String> }               // set label, ^[a-z][a-z0-9_]{0,15}$ (ADR 0057)

/// Everything a harness needs to build one member's start.
pub struct MemberLaunch { pub team: TeamName, pub role: Role, pub address: Address,
                          pub kind: MemberKind, pub cwd: PathBuf, pub session_id: SessionId,
                          pub model: Option<String>, pub effort: Option<String>,
                          pub member_dir: PathBuf,           // $TEAMWIRE_HOME/teams/<team>/members/<role>
                          pub contract_path: PathBuf, pub identity: Vec<(String, String)>,
                          pub otel: OtelConfig, pub teamwire_bin: PathBuf,
                          pub plugins: BTreeMap<String, bool>, // team map overlaid by member map
                          pub context_handoff_tokens: Option<u64> } // member's value, else the team's; a Claude member's compaction backstop is twice its point
/// How full one session's context is: the tokens the model read on the
/// session's newest main-chain request (input + cache read + cache creation),
/// and the window of that model. Every harness computes `tokens` the same way.
pub struct Fill { pub tokens: u64, pub window: u64 }
impl Fill {
    pub fn pct(self) -> u64;                       // whole percent, rounded down; 0 for an empty window
    pub fn handoff_at(self, configured: Option<u64>) -> u64;  // configured, else DEFAULT_HANDOFF_PCT of the window
    pub fn past_handoff(self, configured: Option<u64>) -> bool; // tokens >= handoff_at
    pub fn lead_handoff_at(self, configured: Option<u64>) -> u64; // configured, else DEFAULT_HANDOFF_PCT of the window; never below MIN_LEAD_HANDOFF_TOKENS
}
/// The tool call a session runs now (`Harness::current_tool`).
pub struct ToolRun { pub name: String, pub started_at: Timestamp }

/// Percent of the window at which a member is told to hand off when its spec sets no
/// `context_handoff_tokens`. A rule of thumb: no published measurement backs 40% for Claude.
pub const DEFAULT_HANDOFF_PCT: u64 = 40;

/// The lowest point at which a lead is told to hand off (ADR 0051). A fresh
/// lead holds about 98,000 tokens, so a lower point would tell every new lead
/// to hand off again at once.
pub const MIN_LEAD_HANDOFF_TOKENS: u64 = 200_000;
/// The environment variable, and the `config.toml` key, of the lead's point.
pub const LEAD_HANDOFF_SETTING: &str = "TEAMWIRE_LEAD_HANDOFF_TOKENS";
/// Unset: DEFAULT_HANDOFF_PCT of the window. Off: never told. Tokens: a count;
/// below the floor counts as the floor. Invalid: neither a whole number nor
/// `off` (such as `40%`); counts as Unset, and `teamwire doctor` names it.
pub enum LeadHandoffSetting { Unset, Off, Tokens(u64), Invalid(String) }
impl LeadHandoffSetting {
    pub fn parse(value: Option<&str>) -> Self;  // empty is Unset; `off` in any case; digits may group with `_` or `,`
    pub fn is_off(&self) -> bool;
    pub fn tokens(&self) -> Option<u64>;        // the configured count, when one is given
}
/// The files of one lead's handoff, in `$TEAMWIRE_HOME/handoffs/<old session
/// id>/`. Core builds the paths and touches no file. Nothing deletes the folder.
pub struct HandoffFiles { pub dir: PathBuf,
                          pub document: PathBuf,   // handoff.md, written by the old lead
                          pub noted: PathBuf,      // empty marker: the note was given
                          pub held: PathBuf }      // empty marker: the stop was held once
impl HandoffFiles {
    /// None when the session id is not a safe single path component: only
    /// ASCII letters, digits, `-`, `_` and `.`, and no leading `.`.
    pub fn of(home: &Path, session: &SessionId) -> Option<Self>;
}

/// One command to run in a new pane (`Host::split_run`). `Debug` hides the
/// values in `env`, as `SpawnRequest` does.
pub struct SplitRequest { pub label: String, pub cwd: PathBuf,
                          pub argv: Vec<String>, pub env: Vec<(String, String)> }
impl SplitRequest {
    pub fn lead_label(first_team_title: &str, more_teams: usize) -> String;  // "lead · <title>", "lead · <title> +N"
}
/// What `Harness::lead_start` takes. `old_args` is the old lead process's
/// arguments, `argv[0]` first, or empty when they cannot be read.
/// `new_session` is the id teamwire chose for the new lead.
pub struct LeadStart { pub old_session: SessionId, pub old_args: Vec<String>,
                       pub new_session: SessionId, pub prompt: String }
/// The new lead's argv, with the prompt last, and each old flag not carried,
/// by name only: a value can hold a secret (`pi --api-key`).
/// `warnings` are one line each, printed as `warning:` lines by `teamwire lead
/// --handoff` and its dry run, and never a secret; the contract checks both.
pub struct LeadStartSpec { pub argv: Vec<String>, pub dropped: Vec<String>,
                           pub warnings: Vec<String> }
/// An allow list of old flags. `replaced` names the flags the new start
/// sets itself, such as the session to resume, and whether each takes a
/// value; they are neither carried nor dropped.
pub struct CarryRules<'a> { pub switches: &'a [&'a str], pub with_value: &'a [&'a str],
                            pub replaced: &'a [(&'a str, bool)] }
/// The carried flags in their order, and the dropped flag names. A flag's
/// value is the next word unless that word is a flag. A word that is no flag
/// is never carried or shown. Everything after `--` is the old prompt.
pub fn carry(old_args: &[String], rules: &CarryRules<'_>) -> (Vec<String>, Vec<String>);

impl MemberLaunch { pub fn home_from_member_dir(&self) -> Option<String>;
                     pub fn scratch_dir(&self) -> Option<PathBuf>; }  // <home>/scratch/<team>/<role>; every harness sets TEAMWIRE_SCRATCH to it
/// `<home>/scratch/<team>`: the members' scratch folders, outside the team
/// directory, so the archive never copies them; team close deletes it.
pub fn team_scratch_dir(home: &Path, team: &TeamName) -> PathBuf;
/// One model's tokens in a session so far, in the shape of `data.usage` on
/// `result.delivering`. A kind the harness does not report is 0.
pub struct TokenCounts { pub input: u64, pub output: u64, pub cache_read: u64, pub cache_creation: u64 }
impl TokenCounts { pub fn add(&mut self, other: Self); }  // each count saturating
pub type SessionUsage = BTreeMap<String, TokenCounts>;   // by model name
impl LaunchSpec { pub fn shell_word(text: &str) -> String; }  // single-quotes any word outside [A-Za-z0-9/._-+:=@,]; "%" is quoted
/// What the harness returns. Files are written by launch (mode given) before spawn.
pub struct LaunchSpec { pub argv: Vec<String>,
                        /// Honoured only when the host has `sets_env_at_spawn`.
                        pub env: Vec<(String, String)>,
                        pub files: Vec<LaunchFile>,        // e.g. settings.json, mcp.json
                        pub identity_channel: IdentityChannel }
pub struct LaunchFile { pub path: PathBuf, pub bytes: Vec<u8>, pub mode: u32 }
pub enum IdentityChannel { HarnessConfig, HostEnv }
pub struct SpawnRequest { pub name: Address, pub title: String,
                          pub team_title: String,   // the team row's title
                          pub generation: u32,      // the generation the new session runs as
                          pub cwd: PathBuf,
                          pub argv: Vec<String>, pub env: Vec<(String, String)> }
/// Every host label, so herdr and tmux show one format (team instr, role planrev):
impl SpawnRequest { pub fn workspace_label(&self) -> String;  // "instr · <team title>"
                    pub fn tab_label(&self) -> String;        // "planrev#2 · <title>"; "lead · <team title>"
                    pub fn window_name(&self) -> String;      // "instr/planrev#2"; "instr/lead"
                    pub fn session_name(&self) -> String; }   // "tw_instr"

pub enum TurnEndReply { Continue, Hold(String) }   // Hold carries the note
pub struct Config { pub poll_s: u64, pub silent_polls: u32, pub startup_grace_s: u64,
                    pub setup_timeout_s: u64,                             // 600
                    pub spawn_timeout_s: u64,                             // 150
                    pub default_deadline_s: u64, pub hook_budget_s: u64,  // 40
                    pub hook_timeout_s: u64,                              // 60; the Stop hook timeout of both adapters
                    pub delivering_stale_s: u64,                          // 90
                    pub close_exit_wait_ms: u64,                          // 5000; close's wait for a closed place's processes (CLS-1)
                    pub min_free_bytes: u64, pub otel: OtelConfig }
pub struct OtelConfig { pub endpoint: Option<String>, pub headers: Vec<(String, String)> }

/// The vendor line of a model (`teamwire_core::models`, ADR 0056). Strings:
/// claude, gpt, gemini, grok, deepseek, kimi, glm, qwen, mistral, llama, unknown.
pub enum ModelFamily { Claude, Gpt, Gemini, Grok, Deepseek, Kimi, Glm, Qwen,
                       Mistral, Llama, Unknown }
/// The family of a spec's `model` string, read from the string alone. It
/// reads no file, calls nothing and never guesses: a string the two tables
/// do not place is `Unknown`. The string is lowercased and loses a trailing
/// `:suffix`. An `openrouter/<vendor>/<id>` string is placed by its vendor
/// part alone, after a leading `~`, so `openrouter/openai/o3` is `Gpt`; the
/// `google` vendor also needs an id that starts with `gemini`. Any other
/// string is placed by the start of its `normalise_model` name, such as
/// `claude`, `gpt-` or `deepseek`. Launch and `teamwire models` call
/// `Harness::model_family`, not this function, so a harness can answer for
/// its own models.
pub fn model_family(model: &str) -> ModelFamily;
```

Result types of the service calls (`SendReport`, `InboxItem`, `DoneReport`,
`StatusView`, `LaunchReport`, `VerifyRecord`, `ClosePlan`, `CloseReport`,
`MetricsView`, `ExportReport`) carry exactly the fields of the matching JSON
outputs in `docs/api/mcp.md` and `docs/api/cli.md`.

## 7. Store (`teamwire-store`)

```rust
pub struct Store { /* one rusqlite::Connection, never shared across processes */ }

impl Store {
    /// Creates the directory and database if missing; applies pragmas;
    /// migrates forward; refuses a newer schema (StoreError::SchemaNewer).
    pub fn open(home: &Path, team: &TeamName) -> Result<Store, StoreError>;
    pub fn open_existing(home: &Path, team: &TeamName) -> Result<Store, StoreError>;
    /// For commands that only read (status, log, members, metrics, export,
    /// doctor): `mode=ro`, never creates, never migrates. A database at an
    /// older schema is read as it is, each column a later version added read
    /// as NULL. A newer schema is refused with SchemaNewer, an empty file with
    /// SchemaOlder. Nothing on disk changes (WAT-11).
    pub fn open_read_only(home: &Path, team: &TeamName) -> Result<Store, StoreError>;
    /// Runs `f` inside BEGIN IMMEDIATE; commits on Ok, rolls back on Err.
    pub fn write<T>(&mut self, f: impl FnOnce(&Tx) -> Result<T, StoreError>) -> Result<T, StoreError>;
    pub fn read<T>(&self, f: impl FnOnce(&ReadTx) -> Result<T, StoreError>) -> Result<T, StoreError>;
    pub fn backup_to(&self, path: &Path) -> Result<(), StoreError>;
    pub fn quick_check(&self) -> Result<String, StoreError>;
    pub fn integrity_check(&self) -> Result<String, StoreError>;
    /// Writes the first failed transaction's `store.error` row, best effort.
    pub fn record_failure(&mut self, at: Timestamp, op: &str);
}

pub enum StoreError { Busy, Full { path: PathBuf }, SchemaNewer { db: u32, binary: u32 }, SchemaOlder { db: u32, binary: u32 },
                      NotTeamwire, Corrupt(String), Constraint(String),
                      NoDatabase { path: PathBuf },  // the team has no database file
                      Io(String) }
impl StoreError { pub fn code(&self) -> &'static str; }  // busy, full, schema_newer, schema_older, not_teamwire, corrupt, constraint, no_database, io

pub fn token_hash(token: &str) -> [u8; 32];
pub fn same_hash(a: &[u8], b: &[u8]) -> bool;   // compares every byte, no early exit
// ReadTx::token_matches(role, token) -> Result<Option<bool>, StoreError>: None when role has no
// member that is not removed, or no stored hash; else whether the hashes are the same (same_hash).
// It is the one token check: msg's prove, report's done and the CLI's permission hook use it.

pub struct OpenTeam { pub store: Store, pub row: TeamRow }   // the store opened read-only
pub type TeamRead = Result<Option<OpenTeam>, StoreError>;     // Ok(None): a database with no team row
pub fn open_teams(home: &Path, lead: Option<&SessionId>) -> Result<Vec<(TeamName, TeamRead)>, StoreError>;
```

Permission dialogs (`crates/teamwire-store/src/dialog.rs`, ADR 0052) are three event types and
no table. The `dialog.opened` row's ULID is the dialog id; the other two rows name it as
`subject`. A dialog's state is read from its rows: a `dialog.closed` row gives its outcome;
otherwise `now >= expires_at` gives `Expired`, so a hook that died never leaves a dialog
pending; otherwise a `dialog.answered` row gives `Answered`; otherwise `Pending`.

```rust
pub struct DialogRecord { pub id: Ulid, pub role: Role, pub generation: u32,
                          pub opened_at: Timestamp, pub expires_at: Timestamp,
                          pub request: PermissionRequest, pub state: DialogState,
                          pub answer: Option<PermissionAnswer>,
                          pub answered_at: Option<Timestamp>, pub closed_at: Option<Timestamp> }
pub enum AnswerOutcome { Recorded(Box<DialogRecord>),
                         NoDialog,                  // no dialog.opened row with this id for this role
                         Changed,                   // an earlier session or generation of the member
                         NotPending(DialogState) }  // answered, taken, expired or unanswerable
impl Tx<'_> {
    /// dialog.opened; for a request that is not answerable also dialog.closed `unanswerable`.
    pub fn open_dialog(&self, at: Timestamp, role: &Role, generation: u32,
                       request: &PermissionRequest, expires_at: Timestamp) -> Result<Ulid, StoreError>;
    /// Writes dialog.answered only while the dialog is pending at `at`, of the member's
    /// current session and generation. The check and the write share one BEGIN IMMEDIATE.
    pub fn answer_dialog(&self, at: Timestamp, role: &Role, id: &Ulid,
                         answer: &PermissionAnswer) -> Result<AnswerOutcome, StoreError>;
    /// dialog.closed: `taken` with the answer when one is recorded (even past expires_at),
    /// else `expired`. None, and nothing written, for a dialog already closed.
    pub fn close_dialog(&self, at: Timestamp, id: &Ulid) -> Result<Option<PermissionAnswer>, StoreError>;
}
impl ReadTx<'_> {
    pub fn dialog(&self, id: &Ulid, now: Timestamp) -> Result<Option<DialogRecord>, StoreError>;
    pub fn dialogs_of_role(&self, role: &Role, now: Timestamp) -> Result<Vec<DialogRecord>, StoreError>; // newest first
}
/// Pure, newest first by seq, so the watch reuses its events_of_role rows.
pub fn dialogs_from(events: &[Event], now: Timestamp) -> Vec<DialogRecord>;
```

A command treats `NoDatabase` as "no such team", not as a broken store.
`team_names(home)` lists every team that has a database, sorted by name; it
is the one scan of `<home>/teams` every command uses. `user_key_path(home)`
is `<home>/user.key`, the human user's key (ADR 0040), which belongs to no
team.
`open_teams(home, lead)` builds on it: the open teams, with `lead` only those
that session leads, each with its store and row or the error that reading it
gave. A team whose database went away after the scan is left out. The AFK
start, the Stop hook's checks and both sleep-hold checks (launch, close) use it.

`Tx` offers one typed method per operation the services need, for example
`insert_message`, `insert_received` (another team's message under its own
ULID, ADR 0016), `set_delivery_state`, `next_counter`, `insert_completion`,
`claim_completion_for_delivery`, `claim_completion_for_delivery_with`,
`finish_completion_delivery_with`, `mark_result_received`, `bump_generation`,
`set_member_state`, `create_team_with`, `create_afk_team`, `adopt_lead_in`,
`insert_member_with`, `relaunch_failed_member_with`, `record_event`,
`announce_once` (returns false if the key already exists). The reads on
`ReadTx` include `latest_event(role: Option<&Role>, kind)` (the newest event of
a type; it fails only when that row is damaged), `events_of_kinds(role, kinds)`
(events of those types only), `announcements()` and `message_bodies()` (raw
id, body and hash rows, with no hash refusal).
Every `Tx` method that makes a lifecycle or message change also records the
matching event. A few bookkeeping writes record none, because the closed event
list has no type for them: `set_sleep_hold`, `set_member_place`,
`set_member_session`, `set_member_token_hash`, `set_refs`, `set_gate_told_generation`,
`set_completion_final`, `clear_member_branch`, `set_member_scope` (for `teamwire amend`:
it replaces a member's `check_cmd` and whole `owns` list; its record is the
`CHECK AMENDED:` note stored in the same transaction, ADR 0046), `set_member_marks(role,
quiet, set)` (a member's `quiet` and set label, which launch writes right after it inserts or
relaunches the member and names in the data of that `member.creating` event; `NewMember`
has no field for them), `announce_once`, `heartbeat_watch_lease`, and the
metric-export writes `claim_metric_export` (whose `up_to: Option<u64>` limit stops a claim before an event of an open minute), `commit_metric_export` and
`release_metric_claim`. The list is in the `Tx` doc comment
(`crates/teamwire-store/src/tx.rs`).

The schema is version 13: `crates/teamwire-store/src/schema/v1.sql` to `v13.sql`,
applied in order. Version 2 adds `team.closing_pid_started_at` and
`metric_export.claimed_by_started_at`: the start time of the pid beside
each, NULL when the platform could not read it (ADR 0002 decision 8).
Version 3 changes no table. Version 4 rebuilds `events` (ADR 0020). Version 5
adds `team.afk_budget_ends_at`: when an AFK run's wall-clock budget ends, NULL
on an ordinary team (ADR 0025). Version 6 rebuilds `events` with `member.refused`
in its CHECK list, as version 4 did for `delivery.reentered`, and keeps every
row, every `seq` and the AUTOINCREMENT counter. Version 7 does the same for `member.handoff`
(ADR 0027), whose data holds `generation`, `old_session`, `new_session` (null only in an event an older build wrote for a harness that picked its own id) and `place`. Version 8 does the same for `delivery.recipient_stopped` (ADR 0028).
Version 9 rebuilds `members` with `plan_reviewer` in its `kind` CHECK list
(ADR 0034) and keeps every row. It creates `members_new`, copies the rows,
drops `members` and renames `members_new`, so the foreign keys of
`completions`, `verify_records` and `waivers` still name `members`. Every
migration runs with foreign key checks off and fails, rolling back, when
`PRAGMA foreign_key_check` finds a broken key before the commit. Version 10 (`src/schema/v10.sql`) adds `team.refs`, a JSON array of
`{"provider", "id"}` objects in recorded order that is never NULL and defaults to `[]`, and rebuilds
`events` with the `team.refs_added` type. `TeamRow.refs` is that array; `Tx::set_refs` sets it
when launch creates the team, and `Tx::add_refs(at, refs)` appends the refs not yet held and writes
`team.refs_added`, or writes nothing when none is new (ADR 0038). It refuses with
`StoreError::Constraint`, and appends nothing, when the team would hold more than `REFS_MAX` (20)
refs, because two `--add` runs may each have passed `validate`'s check before either wrote. A
read-only open of an older database reads `refs` as empty. Version 11 (`src/schema/v11.sql`)
rebuilds `events` with the `member.woken` type, as version 10 did, and keeps every row, every
`seq` and the AUTOINCREMENT counter (ADR 0043). The step only goes forward: a database at
version 11 cannot be opened by an older binary. Version 12 (`src/schema/v12.sql`) rebuilds
`events` the same way with three more types, `dialog.opened`, `dialog.answered` and
`dialog.closed`, and adds no table and no column: a dialog's state is read from its events
(ADR 0052). Version 13 (`src/schema/v13.sql`) rebuilds `members` as version 9 did, with
`reconciler` in its `kind` CHECK list and two new last columns: `quiet INTEGER NOT NULL
DEFAULT 0 CHECK (quiet IN (0, 1))` and `set_label TEXT`. Every row keeps its values and takes
`quiet` 0 and `set_label` NULL (ADR 0057, ADR 0058). A read-only open of an older database
shows both as NULL, read as not quiet and in no set. `ReadTx::members` reads its order from
`main.members`, because that open reads `members` through a view, which has no rowid. The
tables and columns are those in
`ARCHITECTURE.md`, "Data model". Constraints
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
    pub home: &'a Path, pub store: &'a mut Store, pub registry: &'a Registry,
    pub platform: &'a dyn Platform, pub clock: &'a dyn Clock, pub config: &'a Config,
}

/// Who a delivery or lookup is for. Resolves to that party's own adapters.
pub enum Party { Lead, Member(Role) }

impl Ctx<'_> {
    /// The harness and host recorded for `party`, from the registry.
    pub fn adapters(&self, party: &Party) -> Result<(&dyn Harness, &dyn Host), DeliveryError>;
}

// teamwire-deliver
pub fn locate(ctx: &Ctx, party: &Party) -> Result<Place, DeliveryError>;  // the locator tests' entry point; delivery calls locate_with inside its own time budget
pub fn deliver_line(ctx: &mut Ctx, party: &Party, line: &OneLine) -> DeliveryOutcome;
pub fn deliver_line_within(ctx: &mut Ctx, party: &Party, line: &OneLine, deadline: Instant)
    -> DeliveryOutcome;
// `&mut Ctx`: a delivery that pressed Enter a second time records `delivery.reentered`.
// DeliveryOutcome = HandedOff | Unconfirmed(DeliveryError) | Undelivered(DeliveryError)

// teamwire-msg
pub fn prove_actor(ctx: &Ctx, env: &CallerEnv) -> Result<Actor, Refusal>;
// A member by its token, the lead by its harness session (ADR 0008), the
// user by `CallerEnv::user_key` against `<home>/user.key`, with no member
// variable set, `token_file` included (ADR 0040).
pub fn adds_lead_copy(actor: &Actor, to: &[Recipient]) -> bool;   // the user's message to a member also goes to the lead
pub fn send(ctx: &mut Ctx, req: SendRequest) -> Result<SendReport, SendError>;
/// `send`, with `also` run inside the transaction that stores the message, so
/// both are saved or neither is; delivery runs after the commit. A closed team
/// runs no `also`; an error from it rolls the message back (`teamwire amend`).
/// Every send reads the message's time inside that write, under the store
/// lock, so the order of the times is the order of the commits (ADR 0046).
pub fn send_with_write(ctx: &mut Ctx, req: SendRequest,
    also: &dyn Fn(&Tx) -> Result<(), StoreError>, on_stored: &mut dyn FnMut(&Stored))
    -> Result<SendReport, SendError>;
pub fn inbox(ctx: &mut Ctx, me: &Role, all: bool) -> Result<Vec<InboxItem>, SendError>;
pub fn ack(ctx: &mut Ctx, me: &Role, ids: &[DisplayId]) -> Result<Vec<AckResult>, SendError>;
pub fn resend(ctx: &mut Ctx, actor: &Actor, id: &DisplayId, to: Option<&[Recipient]>) -> Result<SendReport, SendError>;
pub fn log(ctx: &Ctx, filter: LogFilter) -> Result<Vec<LogRow>, SendError>;

// teamwire-report
pub fn done(ctx: &mut Ctx, me: &Role, status: CompletionStatus, summary: &str) -> Result<DoneReport, DoneError>;
pub fn report_gaps(ctx: &Ctx, member: &Member) -> ReportGaps;   // calls teamwire_verify::report_check; errors → empty gaps
pub fn on_turn_end(ctx: &mut Ctx, me: &Role, t: &TurnEnd) -> TurnEndReply; // Continue | Hold(note)
pub fn on_tool_use(ctx: &mut Ctx, me: &Role) -> Option<String>;            // reminder text

// teamwire-launch
// ValidateEnv carries the adapters, platform, config and lead identity, and no
// store, so a refused launch creates nothing (INV-3; T08).
pub fn validate(spec: &TeamSpec, add: bool, env: &ValidateEnv) -> Result<ValidPlan, Vec<SpecError>>;
pub fn launch(ctx: &mut Ctx, plan: ValidPlan) -> LaunchReport;          // per-member outcomes
pub fn ensure_user_key(platform: &dyn Platform, home: &Path) -> Result<PathBuf, String>; // `teamwire user-key` (ADR 0040)
/// Renders the member's seat rules again from its stored check and owns and
/// writes `<home>/teams/<team>/contract/<role>.md` with `write_atomic`, mode
/// 0644, so a handoff or reopen reads the scope `teamwire amend` set.
pub fn rewrite_contract(platform: &dyn Platform, home: &Path, team: &TeamName,
    member: &Member, teamwire_bin: &Path) -> Result<PathBuf, String>;

// teamwire-verify
pub fn run(ctx: &mut Ctx, role: Option<&Role>, opts: VerifyOpts) -> Result<VerifyReport, VerifyError>;
pub fn waive(ctx: &mut Ctx, role: &Role, flag: &FlagId, reason: &str) -> Result<(), VerifyError>;
// `run` re-reads the member's check and owns in the transaction that stores
// each record; when they changed while the check ran, the record gets the
// flag `scope-changed`, which no waiver clears (ADR 0046).

// teamwire-close
pub fn plan_close(ctx: &Ctx, target: CloseTarget) -> ClosePlan;          // for --dry-run
pub fn close(ctx: &mut Ctx, target: CloseTarget, opts: CloseOpts) -> CloseReport;

// teamwire-watch
pub fn poll(ctx: &mut Ctx, state: &mut WatchMemory) -> Option<OneLine>;  // at most one line
pub fn status_with(ctx: &Ctx, role: Option<&Role>) -> StatusView;       // reads only; role adds that member's resume command

// teamwire-otel
pub fn metrics(ctx: &Ctx, q: MetricsQuery) -> MetricsView;
/// With a team named and no live database, `metrics_at` reads the newest
/// archive whose team row names it. `team_is_known` says whether one exists.
pub fn team_is_known(home: &Path, team: &TeamName) -> bool;
pub fn export_once(ctx: &mut Ctx, exporter: &OtlpConfig) -> Result<ExportReport, ExportError>;
/// The watch's export: once `stopping` is set, a request still waiting gets
/// at most `grace` more.
pub fn export_once_until(ctx: &mut Ctx, exporter: &OtlpConfig, cutoff: Option<StopCutoff>) -> Result<ExportReport, ExportError>;
pub struct StopCutoff<'a> { pub stopping: &'a AtomicBool, pub grace: Duration }
```
