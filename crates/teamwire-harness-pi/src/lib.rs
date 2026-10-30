//! The `Harness` trait for pi (`@earendil-works/pi-coding-agent`; ADR 0029
//! for members, ADR 0030 for the lead): launch argv, the extensions that run
//! teamwire's hooks inside pi, the member's token file, the TUI screen, the
//! files that give liveness, state and context fill, and the lead package.
//!
//! Never: write under pi's directory from teamwire's own process (only
//! teamwire's extensions, inside pi, write state files there); change pi's
//! settings other than by `pi install` and `pi remove`; put a token in an
//! argv; let a member load its folder's own `.pi` resources (`--no-approve`).
//!
//! Invariants: every pi path comes from `PiConfig`, so tests point them at a
//! temporary directory; a member's identity reaches it through its
//! extension, which names the 0600 token file, never the token; a lead is
//! proven only by a state file the lead's extension wrote for a live pi.

mod extension;
mod hook;
mod launch;
mod lead_extension;
pub mod lead_setup;
mod screen;
mod session;
mod startup;

pub use teamwire_core::screen::StartupPoll;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CallerSession, Fill, Harness, HarnessCaps, HarnessError, HarnessId, Host, LaunchSpec, Liveness,
    MemberLaunch, OneLine, OtelConfig, Pid, Place, Platform, Role, ScreenState, SessionId,
    StartupOutcome, TeamName, TrustPolicy, TurnEnd,
};

use crate::lead_extension::LEAD_SESSION_VAR;

/// Where pi keeps its files, and how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiConfig {
    /// `PI_CODING_AGENT_DIR`, else `~/.pi/agent`. teamwire only reads it;
    /// pi and the member's extension write member sessions under it.
    pub agent_dir: PathBuf,
    /// The `pi` program to start.
    pub pi_bin: String,
}

impl PiConfig {
    /// Where members' sessions go: one folder, named in every start with
    /// `--session-dir`, so a session is found from its id alone and stays
    /// out of the user's own session lists.
    #[must_use]
    pub fn sessions_dir(&self) -> PathBuf {
        self.agent_dir.join("sessions").join("teamwire")
    }
}

/// The pi harness adapter (id `pi`).
pub struct Pi {
    platform: Arc<dyn Platform>,
    config: PiConfig,
    startup_poll: StartupPoll,
}

impl Pi {
    pub const ID: &'static str = "pi";

    pub fn new(platform: Arc<dyn Platform>, config: PiConfig) -> Self {
        Self {
            platform,
            config,
            startup_poll: StartupPoll::default(),
        }
    }

    /// How long `answer_startup` waits for the input box.
    #[must_use]
    pub fn with_startup_poll(mut self, poll: StartupPoll) -> Self {
        self.startup_poll = poll;
        self
    }

    pub fn config(&self) -> &PiConfig {
        &self.config
    }
}

impl fmt::Debug for Pi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pi")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Harness for Pi {
    fn id(&self) -> HarnessId {
        HarnessId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HarnessCaps {
        HarnessCaps {
            turn_end_hook: true,
            in_turn_hook: true,
            mcp_client: true,
            choose_session_id: true,
            can_push: false,
            session_env_via_config: true,
            // The lead's extension runs the watch and hands each line to the
            // model as a message (ADR 0030).
            reads_watch_lines: true,
        }
    }

    fn launch_spec(&self, m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        launch::build(&self.config.pi_bin, &self.config.sessions_dir(), m)
    }

    fn resume_spec(&self, m: &MemberLaunch, s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        let mut m = m.clone();
        m.session_id = s.clone();
        launch::build(&self.config.pi_bin, &self.config.sessions_dir(), &m)
    }

    fn current_session(&self) -> Option<CallerSession> {
        self.caller_session(&|name| std::env::var(name).ok())
    }

    /// pi gives the lead's MCP server no session variable, so the lead
    /// extension names the session in the server's environment. That counts
    /// only when the session's pi process is the server's parent, which costs
    /// one `ps`, paid by MCP calls alone. A command run through pi's bash tool
    /// has `PI_SESSION_ID`. Either is a lead only when the lead's extension
    /// wrote its state file.
    fn caller_session(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        let platform = self.platform.as_ref();
        let dir = self.config.sessions_dir();
        if let Some(named) = get(LEAD_SESSION_VAR) {
            let id = SessionId::parse(named.trim()).ok()?;
            let (session, pid) = session::lead_by_id(platform, &dir, &id)?;
            let (parent, _) = platform.parent_of(Pid::new(std::process::id()))?;
            return (parent == pid).then_some(session);
        }
        let id = SessionId::parse(get("PI_SESSION_ID")?.trim()).ok()?;
        session::lead_by_id(platform, &dir, &id).map(|(session, _)| session)
    }

    /// pi opens a session by id from the folder it ran in, which reopen
    /// changes to first.
    fn lead_resume_argv(&self, s: &SessionId) -> Result<Vec<String>, HarnessError> {
        Ok(vec![
            self.config.pi_bin.clone(),
            "--session-id".to_owned(),
            s.as_str().to_owned(),
        ])
    }

    fn lead_setup_missing(&self) -> Option<String> {
        self.current_session().is_none().then(|| {
            "teamwire cannot find a pi lead session here. Run this from the bash tool of a pi \
             session that loaded teamwire's lead package; to add the package, run \
             `teamwire install --harness pi` and start pi again"
                .to_owned()
        })
    }

    fn session_cwd(&self, s: &SessionId) -> Result<Option<PathBuf>, HarnessError> {
        Ok(session::session_cwd(&self.config.sessions_dir(), s))
    }

    fn processes_for_session(
        &self,
        s: &SessionId,
        _timeout: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        Ok(session::processes(
            self.platform.as_ref(),
            &self.config.sessions_dir(),
            s,
        ))
    }

    fn is_alive(&self, s: &SessionId, _timeout: Duration) -> Result<Liveness, HarnessError> {
        Ok(session::liveness(
            self.platform.as_ref(),
            &self.config.sessions_dir(),
            s,
        ))
    }

    fn parse_turn_end(&self, stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        hook::parse_turn_end(stdin)
    }

    fn hold_reply(&self, note: &str) -> Vec<u8> {
        hook::hold_reply(note)
    }

    fn screen_state(&self, tail: &str) -> ScreenState {
        screen::delivery_state(tail)
    }

    fn line_pending(&self, tail: &str, line: &OneLine) -> bool {
        screen::line_pending(tail, line)
    }

    fn reads_input_box(&self) -> bool {
        true
    }

    fn box_holds_text(&self, tail: &str) -> bool {
        screen::box_holds_text(tail)
    }

    fn answer_startup(
        &self,
        host: &dyn Host,
        place: &Place,
        _policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        // `--no-approve` decides folder trust at start, so the spec's
        // `accept_trust` has no dialog to answer.
        startup::answer_startup(host, place, self.startup_poll)
    }

    fn context_fill(
        &self,
        s: &SessionId,
        transcript: Option<&Path>,
    ) -> Result<Option<Fill>, HarnessError> {
        let file = match transcript {
            Some(path) => Some(path.to_path_buf()),
            None => session::session_file(&self.config.sessions_dir(), s),
        };
        Ok(file.and_then(|f| session::context_fill(&f)))
    }

    fn otel_env(
        &self,
        _team: &TeamName,
        _role: &Role,
        _base: &OtelConfig,
    ) -> Vec<(String, String)> {
        Vec::new()
    }
}
