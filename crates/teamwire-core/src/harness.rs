//! The `Harness` trait: one agent program (Claude Code first), and the
//! records a harness produces to start a member.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::OtelConfig;
use crate::dialog::{DialogKind, PermissionDecision, PermissionRequest};
use crate::host::{Host, HostError, PlaceState};
use crate::lead_setup::LeadSetup;
use crate::lead_start::{LeadStart, LeadStartSpec};
use crate::member::{Member, MemberKind};
use crate::models::ModelCatalog;
use crate::names::{Address, HarnessId, Pid, Place, Role, SessionId, TeamName};
use crate::sanitize::OneLine;
use crate::secret::RedactedEnv;
use crate::strings::string_enum;
use crate::tool_call::ToolCall;
use crate::usage::SessionUsage;

pub trait Harness: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> HarnessCaps;
    /// argv, env and files to start one member. Files are written by launch
    /// under the team directory before spawn; nothing outside it.
    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError>;
    /// argv to resume an existing member session with the same per-run
    /// configuration (shown by `status --role`, never run by teamwire).
    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError>;
    /// The caller's own session, read through `get`: the process
    /// environment for a command (`|name| std::env::var(name).ok()`). The
    /// lead's MCP server also answers `MCP_META_<key>` with a string field of
    /// the request's `_meta`, for a harness that names the session only there.
    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession>;
    /// The argv that resumes the lead's own session: a plain resume, with no
    /// per-run files, because the lead loads the teamwire plugin itself. The
    /// default is `Unsupported`.
    fn lead_resume_argv(&self, _s: &SessionId) -> Result<Vec<String>, HarnessError> {
        Err(HarnessError::unsupported())
    }
    /// The command line that starts a fresh interactive lead session like
    /// `old`, with `prompt` as its first prompt. The default is
    /// `Unsupported`.
    fn lead_start(&self, _req: &LeadStart) -> Result<LeadStartSpec, HarnessError> {
        Err(HarnessError::unsupported())
    }
    /// The lead setup `teamwire install`, `uninstall` and `doctor` reach
    /// this harness's lead through (ADR 0031). `None`, the default, for a
    /// harness that cannot lead.
    fn lead_setup(&self) -> Option<&dyn LeadSetup> {
        None
    }
    /// Why the caller's session cannot lead a new team, when the harness can
    /// tell that the lead setup it needs is not loaded there. A pi lead reads
    /// the watch through teamwire's lead extension, so without it no result
    /// would reach the lead. The default is `None`.
    fn lead_setup_missing(&self) -> Option<String> {
        None
    }
    /// The folder a session runs in, read without writing: the live session
    /// first, then the session's own record. `None` when neither names it.
    /// A resume only finds a session from that folder.
    fn session_cwd(&self, _s: &SessionId) -> Result<Option<PathBuf>, HarnessError> {
        Ok(None)
    }
    fn processes_for_session(
        &self,
        s: &SessionId,
        timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError>;
    fn is_alive(&self, s: &SessionId, timeout: Duration) -> Result<Liveness, HarnessError>;
    /// Parse turn-end hook stdin into the neutral form.
    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError>;
    /// Render the hook reply that holds the member once with `note`.
    fn hold_reply(&self, note: &str) -> Vec<u8>;
    /// Parse the lead's tool hook stdin into the neutral form (ADR 0035).
    /// The default is `Unsupported`, and the hook then says nothing.
    fn parse_tool_call(&self, _stdin: &[u8]) -> Result<ToolCall, HarnessError> {
        Err(HarnessError::unsupported())
    }
    /// What kind of dialog the screen `tail` shows (ADR 0052). The default is
    /// `CannotTell`, for a harness that cannot read its own dialogs.
    fn dialog_kind(&self, _tail: &str) -> DialogKind {
        DialogKind::CannotTell
    }
    /// Parse a permission hook's stdin into the neutral request. The default is
    /// `Unsupported`: the harness has no permission hook, and `dialog answer`
    /// refuses its members.
    fn parse_permission_request(&self, _stdin: &[u8]) -> Result<PermissionRequest, HarnessError> {
        Err(HarnessError::unsupported())
    }
    /// The bytes the permission hook prints for `decision`. It allows this one
    /// call, or denies it with the decision's message. It never writes a rule,
    /// changes the input or stops the turn. The default is `Unsupported`.
    fn permission_reply(&self, _decision: &PermissionDecision) -> Result<Vec<u8>, HarnessError> {
        Err(HarnessError::unsupported())
    }
    /// Classify a screen tail: at the input prompt, showing a dialog, or unknown.
    fn screen_state(&self, tail: &str) -> ScreenState;
    /// Whether `line`, just submitted, still sits unsent in the input box
    /// shown by `tail`. The default is `false`: a harness that cannot tell
    /// never gets a second Enter, which can only delay a line, never send it
    /// twice.
    fn line_pending(&self, _tail: &str, _line: &OneLine) -> bool {
        false
    }
    /// Whether the input box shown by `tail` already holds text, such as a
    /// line a person half typed. Delivery waits while it does, on every host.
    /// The default is `false`: a harness that cannot tell types as before.
    fn box_holds_text(&self, _tail: &str) -> bool {
        false
    }
    /// Handle a startup dialog on the member's screen, if the harness has one.
    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError>;
    fn otel_env(&self, team: &TeamName, role: &Role, base: &OtelConfig) -> Vec<(String, String)>;
    /// Put one line into a session without a host. Only called when
    /// `capabilities().can_push`; the default returns `Unsupported`.
    fn push(&self, _s: &SessionId, _line: &OneLine, _timeout: Duration) -> Result<(), HostError> {
        Err(HostError::unsupported())
    }
    /// How full the session's context window is, read without writing.
    /// `transcript` is the path a hook was handed; without it the harness
    /// finds the session's own transcript. `Ok(None)` means no reading: none
    /// yet, or a compaction left none. The caller treats that as unknown,
    /// never as zero. The default is `Ok(None)`.
    fn context_fill(
        &self,
        _s: &SessionId,
        _transcript: Option<&Path>,
    ) -> Result<Option<Fill>, HarnessError> {
        Ok(None)
    }
    /// The tool call the session runs now: the newest call with no result
    /// yet, read without writing from the harness's own session record.
    /// `cwd` is the session's working directory, for a harness that files
    /// its sessions by it. `Ok(None)` means no call runs or the harness
    /// cannot tell. The default is `Ok(None)`.
    fn current_tool(
        &self,
        _s: &SessionId,
        _cwd: Option<&Path>,
    ) -> Result<Option<crate::ToolRun>, HarnessError> {
        Ok(None)
    }
    /// The session's token usage so far, per model, read without writing.
    /// `transcript` is the path a hook was handed; without it the harness
    /// finds the session's own file. `Ok(None)` means no reading. The
    /// default is `Ok(None)`, and the turn-end then reads the transcript it
    /// was handed in the one format the report crate knows.
    fn session_usage(
        &self,
        _s: &SessionId,
        _transcript: Option<&Path>,
    ) -> Result<Option<SessionUsage>, HarnessError> {
        Ok(None)
    }
    /// The models this harness can start a member on, and the effort levels
    /// it takes. A harness whose program lists its models asks the program,
    /// within `timeout`, so the list holds only what this machine can reach;
    /// any other carries a fixed list. The default is `Unsupported`.
    fn models(&self, _timeout: Duration) -> Result<ModelCatalog, HarnessError> {
        Err(HarnessError::unsupported())
    }
}

/// The share of the window at which a member is told to hand off when its
/// spec sets no `context_handoff_tokens`. A practitioner's rule of thumb:
/// no published measurement backs 40% for Claude or for coding agents.
pub const DEFAULT_HANDOFF_PCT: u64 = 40;

/// The lowest `context_handoff_tokens` that makes sense for Claude Code. A
/// live check on 2026-10-21 (sonnet, three plugins off) found a fresh session
/// already holds 61,000 to 64,000 tokens before its first tool call, so a
/// lower point is passed at once and every handoff starts a session that is
/// told to hand off again. It is also the floor of `autoCompactWindow`
/// (<https://code.claude.com/docs/en/settings-reference#autocompactwindow>).
pub const MIN_HANDOFF_TOKENS: u64 = 100_000;

/// How full one session's context is: the tokens the model read on the
/// session's newest main-chain request, and the window of that model. Every
/// harness computes `tokens` the same way, so a percentage means one thing
/// across harnesses and never copies a harness's own display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    pub tokens: u64,
    pub window: u64,
}

impl Fill {
    /// Whole percent of the window, rounded down; 0 for an empty window.
    #[must_use]
    pub fn pct(self) -> u64 {
        self.tokens
            .saturating_mul(100)
            .checked_div(self.window)
            .unwrap_or(0)
    }

    /// The token count at which the member is told to hand off: the spec's
    /// `context_handoff_tokens` when set, else `DEFAULT_HANDOFF_PCT` of the
    /// window.
    #[must_use]
    pub fn handoff_at(self, configured: Option<u64>) -> u64 {
        configured.unwrap_or_else(|| self.window.saturating_mul(DEFAULT_HANDOFF_PCT) / 100)
    }

    /// Whether the session has reached its handoff point.
    #[must_use]
    pub fn past_handoff(self, configured: Option<u64>) -> bool {
        self.tokens >= self.handoff_at(configured)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "a capability record is a set of independent yes/no facts (ADR 0004 point 5)"
)]
pub struct HarnessCaps {
    pub turn_end_hook: bool,
    pub in_turn_hook: bool,
    pub mcp_client: bool,
    pub choose_session_id: bool,
    pub can_push: bool,
    /// Environment for the session can be set in a per-run
    /// config file (Claude Code `settings.json` `env`).
    pub session_env_via_config: bool,
    /// A lead on this harness can run `teamwire watch` and read each line as
    /// it is printed (Claude Code's Monitor tool). Without it the lead needs
    /// `teamwire watch --deliver`, which types the lines into its pane, so
    /// launch refuses such a lead when it cannot be located (ADR 0023).
    pub reads_watch_lines: bool,
    /// How a lead on this harness starts `teamwire watch`. Every text that
    /// tells a lead to start a watch is worded from it.
    pub watch_start: WatchStart,
    /// The full sentence that tells a lead on this harness to load the
    /// teamwire skill, as the harness's lead file words it. Empty for a
    /// harness that cannot lead. Never read back from JSON.
    #[serde(skip_deserializing)]
    pub load_skill: &'static str,
    /// `Harness::box_holds_text` can tell anything. Delivery on a host that
    /// guards dialogs reads the screen only when it can, so a harness that
    /// cannot costs no extra read. Delivery gives a harness that reads the
    /// box its screens from `Host::read_tail_styled`, in `screen_state` and
    /// `box_holds_text` both, so they must parse style codes.
    pub reads_input_box: bool,
    /// A lead on this harness can start an AFK run: its skill carries the
    /// AFK flow, and its pane can be proven to reach the lead (AFK-2).
    pub runs_afk: bool,
}

/// A turn-end hook's input, in neutral form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnEnd {
    pub session: SessionId,
    pub subagent: bool,
    pub hold_already_active: bool,
    pub final_message: Option<String>,
    pub transcript: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Liveness {
    Alive(PlaceState),
    Dead,
    Unknown,
}

string_enum! {
    /// How a lead starts `teamwire watch` and gets its lines.
    pub enum WatchStart {
        /// The lead runs the command under a tool that hands it each line
        /// as the command prints it.
        MonitoredCommand => "monitored_command",
        /// The lead runs the command in the background with `--deliver`, and
        /// the watch types each line into the lead's pane. No harness starts
        /// the watch this way now; ADR 0054 decision 7 lists it for removal.
        BackgroundCommand => "background_command",
        /// The lead calls a tool of teamwire's lead extension, which runs the
        /// watch and hands the lead each line (ADR 0036).
        ExtensionTool => "extension_tool",
    }
}

string_enum! {
    pub enum ScreenState {
        InputPrompt => "input_prompt",
        Dialog => "dialog",
        Unknown => "unknown",
    }
}

string_enum! {
    pub enum StartupOutcome {
        NoDialog => "no_dialog",
        AnsweredPlainTrust => "answered_plain_trust",
        RefusedEscalated => "refused_escalated",
    }
}

string_enum! {
    pub enum SessionKind {
        Interactive => "interactive",
        Background => "background",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallerSession {
    pub id: SessionId,
    pub kind: SessionKind,
}

string_enum! {
    /// From the spec's `accept_trust`.
    pub enum TrustPolicy {
        Accept => "accept",
        Refuse => "refuse",
    }
}

/// Everything a harness needs to build one member's start. `identity` holds
/// the member's token, so `Debug` hides secret values.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberLaunch {
    pub team: TeamName,
    pub role: Role,
    pub address: Address,
    pub kind: MemberKind,
    pub cwd: PathBuf,
    pub session_id: SessionId,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `$TEAMWIRE_HOME/teams/<team>/members/<role>`.
    pub member_dir: PathBuf,
    pub contract_path: PathBuf,
    pub identity: Vec<(String, String)>,
    pub otel: OtelConfig,
    pub teamwire_bin: PathBuf,
    /// Plugin id to on/off, the team's map overlaid by the member's. A harness
    /// without plugins ignores it.
    #[serde(default)]
    pub plugins: BTreeMap<String, bool>,
    /// The team's `context_handoff_tokens` overlaid by the member's. A Claude
    /// member's compaction backstop is set from it (and from the model).
    #[serde(default)]
    pub context_handoff_tokens: Option<u64>,
}

impl MemberLaunch {
    /// The launch of a member that already ran, for `Harness::resume_spec`.
    /// Its settings, MCP configuration and contract are already on disk under
    /// `team_dir`, so only their paths matter and the token is a placeholder.
    /// `model` and `effort` are the ones recorded when the member started.
    pub fn for_resume(
        team_dir: &Path,
        team: &TeamName,
        member: &Member,
        session: &SessionId,
        (model, effort): (Option<String>, Option<String>),
    ) -> Self {
        Self {
            team: team.clone(),
            role: member.role.clone(),
            address: Address::of(team, &member.role),
            kind: member.kind,
            cwd: member.cwd.clone(),
            session_id: session.clone(),
            model,
            effort,
            member_dir: team_dir.join("members").join(member.role.as_str()),
            contract_path: team_dir
                .join("contract")
                .join(format!("{}.md", member.role)),
            identity: vec![("TEAMWIRE_TOKEN".to_owned(), "-".to_owned())],
            otel: OtelConfig::default(),
            teamwire_bin: PathBuf::from("teamwire"),
            plugins: BTreeMap::new(),
            // A resume never rewrites the per-run files.
            context_handoff_tokens: None,
        }
    }
}

impl MemberLaunch {
    /// `$TEAMWIRE_HOME/teams/<team>/members/<role>` names its home four
    /// levels up.
    #[must_use]
    pub fn home_from_member_dir(&self) -> Option<String> {
        let home = self.member_dir.ancestors().nth(4)?;
        (!home.as_os_str().is_empty()).then(|| home.display().to_string())
    }

    /// The member's scratch folder, `$TEAMWIRE_HOME/scratch/<team>/<role>`,
    /// from the home `member_dir` names. Launch makes it, and every harness
    /// hands its path to the member as `TEAMWIRE_SCRATCH`.
    #[must_use]
    pub fn scratch_dir(&self) -> Option<PathBuf> {
        let home = self.home_from_member_dir()?;
        Some(team_scratch_dir(Path::new(&home), &self.team).join(self.role.as_str()))
    }
}

/// The folder of a team's member scratch folders, `<home>/scratch/<team>`.
/// It is outside `teams/<team>/`, so the archive never copies it; team close
/// deletes it.
#[must_use]
pub fn team_scratch_dir(home: &Path, team: &TeamName) -> PathBuf {
    home.join("scratch").join(team.as_str())
}

impl LaunchSpec {
    /// `text` as one shell word, for a command a person or `sh -c` reads: a
    /// word with anything but plain characters is single-quoted, each `'`
    /// written `'\''`.
    #[must_use]
    pub fn shell_word(text: &str) -> String {
        let plain = !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-+:=@,".contains(c));
        if plain {
            text.to_owned()
        } else {
            format!("'{}'", text.replace('\'', "'\\''"))
        }
    }

    /// `argv` as one shell command line, for a command teamwire only prints.
    pub fn shell_line(argv: &[String]) -> String {
        argv.iter()
            .map(|text| Self::shell_word(text))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What the harness returns. Files are written by launch (mode given) before spawn.
/// `env` and the files can hold the token, so `Debug` hides secret values and
/// prints each file's size, never its bytes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub argv: Vec<String>,
    /// Honoured only when the host has `sets_env_at_spawn`.
    pub env: Vec<(String, String)>,
    /// For example `settings.json` and `mcp.json`.
    pub files: Vec<LaunchFile>,
    pub identity_channel: IdentityChannel,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub mode: u32,
}

string_enum! {
    /// How a member's identity variables reach it (ADR 0008).
    pub enum IdentityChannel {
        HarnessConfig => "harness_config",
        HostEnv => "host_env",
    }
}

/// What the turn-end hook tells the harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnEndReply {
    Continue,
    /// Carries the note.
    Hold(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{code}: {detail}")]
pub struct HarnessError {
    pub code: HarnessErrorCode,
    pub detail: String,
}

impl HarnessError {
    pub fn new(code: HarnessErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// The adapter does not offer this call.
    pub fn unsupported() -> Self {
        Self::new(
            HarnessErrorCode::Unsupported,
            "not supported by this adapter",
        )
    }
}

string_enum! {
    pub enum HarnessErrorCode {
        /// The harness's session list or file does not know the session.
        NotFound => "not_found",
        /// A listing or read ran past its timeout.
        Timeout => "timeout",
        /// Hook input or a harness file could not be parsed.
        BadInput => "bad_input",
        /// The member cannot be started as asked (for example no identity channel).
        Refused => "refused",
        /// The call went through the host, and the host failed.
        Host => "host",
        Io => "io",
        Unsupported => "unsupported",
        Other => "other",
    }
}

impl From<HostError> for HarnessError {
    fn from(e: HostError) -> Self {
        Self::new(HarnessErrorCode::Host, e.to_string())
    }
}

impl fmt::Debug for MemberLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemberLaunch")
            .field("team", &self.team)
            .field("role", &self.role)
            .field("address", &self.address)
            .field("kind", &self.kind)
            .field("cwd", &self.cwd)
            .field("session_id", &self.session_id)
            .field("model", &self.model)
            .field("effort", &self.effort)
            .field("member_dir", &self.member_dir)
            .field("contract_path", &self.contract_path)
            .field("identity", &RedactedEnv(&self.identity))
            .field("otel", &self.otel)
            .field("teamwire_bin", &self.teamwire_bin)
            .field("plugins", &self.plugins)
            .field("context_handoff_tokens", &self.context_handoff_tokens)
            .finish()
    }
}

impl fmt::Debug for LaunchSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchSpec")
            .field("argv", &self.argv)
            .field("env", &RedactedEnv(&self.env))
            .field("files", &self.files)
            .field("identity_channel", &self.identity_channel)
            .finish()
    }
}

impl fmt::Debug for LaunchFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchFile")
            .field("path", &self.path)
            .field("bytes", &format_args!("<{} bytes>", self.bytes.len()))
            .field("mode", &format_args!("{:o}", self.mode))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_defaults_to_forty_percent_of_the_window() {
        let f = Fill {
            tokens: 399_999,
            window: 1_000_000,
        };
        assert_eq!(f.handoff_at(None), 400_000);
        assert!(!f.past_handoff(None));
        assert!(
            Fill {
                tokens: 400_000,
                ..f
            }
            .past_handoff(None)
        );
        assert_eq!(f.handoff_at(Some(120_000)), 120_000);
        assert!(f.past_handoff(Some(120_000)));
        assert_eq!(
            Fill {
                tokens: 720_091,
                window: 1_000_000
            }
            .pct(),
            72
        );
        assert_eq!(
            Fill {
                tokens: 5,
                window: 0
            }
            .pct(),
            0
        );
    }

    #[test]
    fn shell_word_quotes_only_what_needs_it() {
        assert_eq!(LaunchSpec::shell_word("/opt/x/teamwire"), "/opt/x/teamwire");
        assert_eq!(LaunchSpec::shell_word("a=b,c@d"), "a=b,c@d");
        assert_eq!(LaunchSpec::shell_word(""), "''");
        assert_eq!(LaunchSpec::shell_word("/my dir/tw"), "'/my dir/tw'");
        assert_eq!(LaunchSpec::shell_word("it's"), "'it'\\''s'");
        assert_eq!(LaunchSpec::shell_word("50%"), "'50%'");
    }

    #[test]
    fn the_scratch_folder_is_beside_teams_not_inside_the_team() {
        let team = TeamName::parse("demo").unwrap();
        let role = Role::parse("api").unwrap();
        let launch = MemberLaunch {
            address: Address::of(&team, &role),
            team: team.clone(),
            role,
            kind: MemberKind::Implementer,
            cwd: PathBuf::from("/work"),
            session_id: SessionId::parse("s1").unwrap(),
            model: None,
            effort: None,
            member_dir: PathBuf::from("/h/teams/demo/members/api"),
            contract_path: PathBuf::from("/h/teams/demo/contract/api.md"),
            identity: Vec::new(),
            otel: OtelConfig::default(),
            teamwire_bin: PathBuf::from("teamwire"),
            plugins: BTreeMap::new(),
            context_handoff_tokens: None,
        };
        assert_eq!(
            launch.scratch_dir(),
            Some(PathBuf::from("/h/scratch/demo/api"))
        );
        assert_eq!(
            team_scratch_dir(Path::new("/h"), &team),
            PathBuf::from("/h/scratch/demo")
        );
    }

    #[test]
    fn shell_line_joins_quoted_words() {
        let argv = ["claude".to_owned(), "a b".to_owned()];
        assert_eq!(LaunchSpec::shell_line(&argv), "claude 'a b'");
    }
}
