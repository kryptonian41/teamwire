//! The `Host` trait: the place programs run (herdr, tmux, none).

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::names::{Address, HostId, Pid, Place, SessionId};
use crate::sanitize::OneLine;
use crate::secret::RedactedEnv;
use crate::split::SplitRequest;
use crate::strings::string_enum;

pub trait Host: Send + Sync {
    fn id(&self) -> HostId;
    fn capabilities(&self) -> HostCaps;
    /// Create a new place for a member and run `argv` in it.
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError>;
    /// Type `text` and submit it. `Ok` means the host accepted it, no more.
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError>;
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError>;
    /// Who occupies `place`: a session id if the host knows it, else process ids.
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError>;
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError>;
    /// Like `read_tail`, but keeps the terminal's SGR style codes, so a reader
    /// can tell dim placeholder text from text a person typed. The default is
    /// the plain tail: a host that cannot give styles reads as before.
    fn read_tail_styled(
        &self,
        place: &Place,
        lines: u16,
        timeout: Duration,
    ) -> Result<String, HostError> {
        self.read_tail(place, lines, timeout)
    }
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError>;
    /// Close `place` only if the agent in it is `expected`. A place that
    /// holds another agent, or none, is left alone and reported `NotFound`:
    /// a place id can be reused after the member's agent died (CLS-7).
    fn close(&self, place: &Place, expected: &Address, timeout: Duration) -> Result<(), HostError>;
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError>;
    /// Names of every live agent the host knows, from any tool (for LCH-25).
    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError>;
    /// The environment variable a process uses to name its own place, if any
    /// (herdr: "`HERDR_PANE_ID`").
    fn place_env_var(&self) -> Option<&'static str>;
    /// The place id one process names for itself, given a reader of that
    /// process's environment. The default is the value of `place_env_var`. A
    /// host whose place id needs more than that one variable (tmux also needs
    /// the server) overrides it, so the locator never learns the id format.
    fn place_from_env(&self, read: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        read(self.place_env_var()?)
    }
    /// Splits `beside` and runs `req.argv` in the new pane, which gets
    /// `req.label`. The new pane holds no teamwire agent name. The default
    /// is `Unsupported`.
    fn split_run(
        &self,
        _beside: &Place,
        _req: &SplitRequest,
        _timeout: Duration,
    ) -> Result<Place, HostError> {
        Err(HostError::unsupported())
    }
    /// Closes a pane that holds no teamwire agent name. The method checks
    /// nothing about the occupant. The caller may close only a pane it opened
    /// with `split_run` in the same command, or a pane whose occupant it has
    /// proven with the locator right before this call. A pane that is already
    /// gone counts as closed. The default is `Unsupported`.
    fn close_pane(&self, _place: &Place, _timeout: Duration) -> Result<(), HostError> {
        Err(HostError::unsupported())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "a capability record is a set of independent yes/no facts (ADR 0004 point 5)"
)]
pub struct HostCaps {
    pub can_type: bool,
    pub reports_state: bool,
    pub knows_session_of_place: bool,
    pub submit_ack: SubmitAck,
    /// The host can give the started program environment
    /// variables (tmux `new-window -e`). herdr cannot.
    pub sets_env_at_spawn: bool,
    /// The host itself refuses to type into a session that
    /// shows a dialog (herdr `agent_blocked`). tmux does not.
    pub guards_dialogs: bool,
    /// The host offers `split_run` and `close_pane`: it can run a plain
    /// command in a new pane beside another, and close that pane.
    pub splits_pane: bool,
}

string_enum! {
    /// What `Ok(())` from `Host::submit` proves.
    pub enum SubmitAck {
        None => "none",
        Submitted => "submitted",
        StateChange => "state_change",
    }
}

string_enum! {
    pub enum PlaceState {
        Idle => "idle",
        Working => "working",
        Blocked => "blocked",
        Gone => "gone",
        Unknown => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occupant {
    pub session: Option<SessionId>,
    pub pids: Vec<Pid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{code}: {detail}")]
pub struct HostError {
    pub code: HostErrorCode,
    pub detail: String,
}

impl HostError {
    pub fn new(code: HostErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// The adapter does not offer this call.
    pub fn unsupported() -> Self {
        Self::new(HostErrorCode::Unsupported, "not supported by this adapter")
    }
}

string_enum! {
    pub enum HostErrorCode {
        NotReady => "not_ready",
        Busy => "busy",
        Blocked => "blocked",
        NotFound => "not_found",
        Stalled => "stalled",
        Timeout => "timeout",
        BadArgument => "bad_argument",
        Unsupported => "unsupported",
        Other => "other",
    }
}

/// What a host needs to start one member in a new place. `Debug` hides
/// secret values in `env`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub name: Address,
    pub title: String,
    /// The team's title, shown in the herdr workspace label and in the
    /// lead's tab label.
    pub team_title: String,
    /// The member's generation the new session runs as: 1 at launch, the
    /// new number after a handoff, the stored one on reopen.
    pub generation: u32,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Between the name and the title in a herdr label.
const LABEL_SEPARATOR: &str = " \u{b7} ";

/// Every label a host gives a place is built here, so herdr and tmux cannot
/// show a team or a member in two formats.
impl SpawnRequest {
    /// The herdr workspace of a team: `instr · Fix lead and member prompts`.
    pub fn workspace_label(&self) -> String {
        format!("{}{LABEL_SEPARATOR}{}", self.name.team(), self.team_title)
    }

    /// The herdr tab: `planrev · Review the fix plan`, `planrev#2 · ...`
    /// after a handoff, and `lead · <team title>` for the lead.
    pub fn tab_label(&self) -> String {
        let title = if self.is_lead() {
            &self.team_title
        } else {
            &self.title
        };
        format!("{}{LABEL_SEPARATOR}{title}", self.seat())
    }

    /// The tmux window: `instr/planrev`, `instr/planrev#2`, `instr/lead`. It
    /// holds no title: the status bar has little room.
    pub fn window_name(&self) -> String {
        format!("{}/{}", self.name.team(), self.seat())
    }

    /// The detached tmux session of the team: `tw_instr`.
    pub fn session_name(&self) -> String {
        format!("tw_{}", self.name.team())
    }

    fn is_lead(&self) -> bool {
        self.name.role() == "lead"
    }

    /// The role, with `#<generation>` from the second generation on. The
    /// lead has no generations.
    fn seat(&self) -> String {
        let role = self.name.role();
        if self.generation > 1 && !self.is_lead() {
            format!("{role}#{}", self.generation)
        } else {
            role.to_owned()
        }
    }
}

impl SplitRequest {
    /// The pane of a fresh lead after a lead handoff (ADR 0051), labelled
    /// like a lead's tab: `lead · <team title>` for one team, and
    /// `lead · <team title> +N` when the lead leads N more teams. The title is
    /// the first led team's by name.
    pub fn lead_label(first_team_title: &str, more_teams: usize) -> String {
        let label = format!("lead{LABEL_SEPARATOR}{first_team_title}");
        if more_teams == 0 {
            label
        } else {
            format!("{label} +{more_teams}")
        }
    }
}

impl fmt::Debug for SpawnRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpawnRequest")
            .field("name", &self.name)
            .field("title", &self.title)
            .field("team_title", &self.team_title)
            .field("generation", &self.generation)
            .field("cwd", &self.cwd)
            .field("argv", &self.argv)
            .field("env", &RedactedEnv(&self.env))
            .finish()
    }
}

/// One key for `Host::send_keys`. Each host adapter maps these to its own
/// key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    CtrlC,
    /// One printable character, such as the `1` that picks a dialog option.
    Char(char),
}

/// The host for "no multiplexer" (id "none"). Every capability false; every
/// writing call returns `HostError` { code: `NotFound`, .. } and does nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHost;

impl NoHost {
    pub const ID: &'static str = "none";

    fn not_found() -> HostError {
        HostError::new(HostErrorCode::NotFound, "no host: there is no place")
    }
}

impl Host for NoHost {
    fn id(&self) -> HostId {
        HostId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HostCaps {
        HostCaps {
            can_type: false,
            reports_state: false,
            knows_session_of_place: false,
            submit_ack: SubmitAck::None,
            sets_env_at_spawn: false,
            guards_dialogs: false,
            splits_pane: false,
        }
    }

    fn spawn(&self, _req: &SpawnRequest, _timeout: Duration) -> Result<Place, HostError> {
        Err(Self::not_found())
    }

    fn submit(&self, _place: &Place, _text: &OneLine, _timeout: Duration) -> Result<(), HostError> {
        Err(Self::not_found())
    }

    fn state(&self, _place: &Place, _timeout: Duration) -> Result<PlaceState, HostError> {
        Err(Self::not_found())
    }

    fn occupant(&self, _place: &Place, _timeout: Duration) -> Result<Occupant, HostError> {
        Err(Self::not_found())
    }

    fn read_tail(
        &self,
        _place: &Place,
        _lines: u16,
        _timeout: Duration,
    ) -> Result<String, HostError> {
        Err(Self::not_found())
    }

    fn send_keys(
        &self,
        _place: &Place,
        _keys: &[Key],
        _timeout: Duration,
    ) -> Result<(), HostError> {
        Err(Self::not_found())
    }

    fn close(
        &self,
        _place: &Place,
        _expected: &Address,
        _timeout: Duration,
    ) -> Result<(), HostError> {
        Err(Self::not_found())
    }

    fn list(&self, _timeout: Duration) -> Result<Vec<Place>, HostError> {
        Ok(Vec::new())
    }

    fn live_agent_names(&self, _timeout: Duration) -> Result<Vec<String>, HostError> {
        Ok(Vec::new())
    }

    fn place_env_var(&self) -> Option<&'static str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::TeamName;

    fn request(name: Address, title: &str, generation: u32) -> SpawnRequest {
        SpawnRequest {
            name,
            title: title.to_owned(),
            team_title: "Fix lead and member prompts".to_owned(),
            generation,
            cwd: PathBuf::from("/w"),
            argv: Vec::new(),
            env: Vec::new(),
        }
    }

    fn member(generation: u32) -> SpawnRequest {
        let name = Address::parse("tw_instr_planrev").unwrap();
        request(name, "Review the fix plan", generation)
    }

    #[test]
    fn a_first_generation_member_is_labelled_role_then_title() {
        let req = member(1);
        assert_eq!(
            req.workspace_label(),
            "instr \u{b7} Fix lead and member prompts"
        );
        assert_eq!(req.tab_label(), "planrev \u{b7} Review the fix plan");
        assert_eq!(req.window_name(), "instr/planrev");
        assert_eq!(req.session_name(), "tw_instr");
    }

    #[test]
    fn a_later_generation_carries_its_number() {
        let req = member(2);
        assert_eq!(req.tab_label(), "planrev#2 \u{b7} Review the fix plan");
        assert_eq!(req.window_name(), "instr/planrev#2");
        assert_eq!(
            req.workspace_label(),
            "instr \u{b7} Fix lead and member prompts"
        );
        assert_eq!(req.session_name(), "tw_instr");
    }

    #[test]
    fn the_lead_is_labelled_with_the_team_title_and_no_number() {
        let lead = Address::lead(&TeamName::parse("instr").unwrap());
        let req = request(lead, "lead", 3);
        assert_eq!(req.tab_label(), "lead \u{b7} Fix lead and member prompts");
        assert_eq!(req.window_name(), "instr/lead");
        assert_eq!(req.session_name(), "tw_instr");
    }
}
