//! What close and removal must know about a member before they touch it:
//! whether it is busy, whether its worktree holds uncommitted work, and
//! whether the team's close lock is held. Each answer is a refusal text, so
//! the plan, the first check and every re-check under the lock say the same
//! thing (CLS-3).

use std::time::Duration;

use teamwire_core::{HostErrorCode, Member, MemberState, Mode, PlaceState, Role, TeamState};
use teamwire_store::{Ctx, TeamRow};

use crate::git;

/// Asking a host about one place is a quick call; a host that does not
/// answer in this long is "unknown", and unknown refuses.
const HOST_TIMEOUT: Duration = Duration::from_secs(10);

/// The brief's copy in a worktree. Launch keeps it out of git; close ignores
/// it wherever it looks (LCH-24).
const TASK_FILE: &str = "TASK.md";

/// How busy a member is, as its own host reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Busy {
    /// Nothing is running, or the place is already gone.
    Idle,
    /// A turn is running: closing the place loses it.
    Working,
    /// A dialog waits for an answer.
    Blocked,
    /// The host could not say. Close refuses rather than guess.
    Unknown(String),
}

/// The members that still count: a removed member was taken down earlier.
pub(crate) fn live(members: &[Member]) -> impl Iterator<Item = &Member> {
    members.iter().filter(|m| m.state != MemberState::Removed)
}

/// Asks the member's own host about the member's own place (CLS-7).
pub(crate) fn busy_state(ctx: &Ctx<'_>, member: &Member) -> Busy {
    let Some(place) = &member.place else {
        return Busy::Idle;
    };
    let (_, host) = match ctx.adapters(&teamwire_store::Party::Member(member.role.clone())) {
        Ok(pair) => pair,
        Err(e) => return Busy::Unknown(e.to_string()),
    };
    match host.state(place, HOST_TIMEOUT) {
        Ok(PlaceState::Idle | PlaceState::Gone) => Busy::Idle,
        Ok(PlaceState::Working) => Busy::Working,
        Ok(PlaceState::Blocked) => Busy::Blocked,
        Ok(PlaceState::Unknown) => Busy::Unknown("the host reports no state".to_owned()),
        Err(e) if e.code == HostErrorCode::NotFound => Busy::Idle,
        Err(e) => Busy::Unknown(e.to_string()),
    }
}

/// Why this member must not be closed now, if it is busy or unreadable.
/// `stop_allowed` is `--stop` on a single removal: a running turn or a dialog
/// is then accepted, but an unknown state never is.
pub(crate) fn busy_refusal(ctx: &Ctx<'_>, member: &Member, stop_allowed: bool) -> Option<String> {
    let role = &member.role;
    match busy_state(ctx, member) {
        Busy::Idle => None,
        Busy::Working if stop_allowed => None,
        Busy::Blocked if stop_allowed => None,
        Busy::Working => Some(format!(
            "{role} is busy: its current turn is lost if its place closes. Wait for it, or \
             remove it alone with --role {role} --stop"
        )),
        Busy::Blocked => Some(format!(
            "{role} is showing a dialog and waits for an answer. Answer it, or remove it \
             alone with --role {role} --stop"
        )),
        Busy::Unknown(why) => Some(format!(
            "cannot tell whether {role} is busy ({why}); refusing rather than guess"
        )),
    }
}

/// The uncommitted paths in a worktree member's checkout, ignoring `TASK.md`.
/// An error is text: git that cannot say what is uncommitted is a refusal,
/// never "clean".
pub(crate) fn dirty_paths(ctx: &Ctx<'_>, member: &Member) -> Result<Vec<String>, String> {
    if member.mode != Mode::Worktree || !member.cwd.is_dir() {
        return Ok(Vec::new());
    }
    let out = git::run(
        ctx.platform,
        &member.cwd,
        &["status", "--porcelain", "--untracked-files=normal"],
    )?;
    if !out.success() {
        return Err(format!(
            "git status in {}: {}",
            member.cwd.display(),
            git::failure_text(&out)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty() && !l.ends_with(TASK_FILE))
        .map(str::to_owned)
        .collect())
}

/// Why this member's checkout must not be removed now, if it has uncommitted
/// work or git cannot say.
pub(crate) fn dirty_refusal(ctx: &Ctx<'_>, member: &Member) -> Option<String> {
    match dirty_paths(ctx, member) {
        Ok(paths) if paths.is_empty() => None,
        Ok(paths) => Some(format!(
            "{} has {} uncommitted path(s) in {}. Commit or copy that work out first; \
             teamwire never forces a removal",
            member.role,
            paths.len(),
            member.cwd.display()
        )),
        Err(e) => Some(format!(
            "cannot tell whether {} has uncommitted work ({e}); refusing rather than guess",
            member.role
        )),
    }
}

/// Every busy and dirty refusal for `members`, busy first.
pub(crate) fn member_refusals(ctx: &Ctx<'_>, members: &[Member]) -> Vec<String> {
    let mut out: Vec<String> = live(members)
        .filter_map(|m| busy_refusal(ctx, m, false))
        .collect();
    out.extend(live(members).filter_map(|m| dirty_refusal(ctx, m)));
    out
}

/// Why no close may start now, from the team row alone.
pub(crate) fn lock_refusal(ctx: &Ctx<'_>, team: &TeamRow) -> Option<String> {
    match team.state {
        TeamState::Open => None,
        TeamState::Closed => None,
        TeamState::Closing => Some(match team.closing_pid {
            Some(pid) if ctx.platform.pid_alive(pid) => {
                format!("a close is already running (pid {pid}); wait for it")
            }
            Some(pid) => format!(
                "a close left its lock (pid {pid} is gone). Clear it with `teamwire close \
                 {} --clear-stale-lock`, then run the close again",
                team.name
            ),
            None => format!(
                "a close left its lock. Clear it with `teamwire close {} --clear-stale-lock`",
                team.name
            ),
        }),
    }
}

/// The role named in a refusal about a member that is not in the team.
pub(crate) fn no_such_member(role: &Role, team: &TeamRow) -> String {
    format!("no member {role} in team {}", team.name)
}
