//! What close and removal must know about a member before they touch it:
//! whether it is busy, whether its worktree holds uncommitted work, and
//! whether the team's close lock is held. Each answer is a refusal text, so
//! the plan, the first check and every re-check under the lock say the same
//! thing (CLS-3).

use std::time::Duration;

use teamwire_core::platform::ProcessRef;
use teamwire_core::{
    Address, HostErrorCode, Member, MemberState, Mode, PlaceState, Role, TeamName, TeamState,
};
use teamwire_store::{Ctx, TeamRow};

use crate::git;

/// Asking a host about one place is a quick call; a host that does not
/// answer in this long is "unknown", and unknown refuses.
const HOST_TIMEOUT: Duration = Duration::from_secs(10);

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
        .filter(|l| !l.is_empty() && !teamwire_verify::is_task_file_status(l))
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

/// Why close must not go on for a member that has no recorded place while
/// its agent may still run. A launch killed inside the host's start call
/// leaves the member `creating` with no place, and the agent running under
/// the member's address (LCH-3). Close has no place to close it through, so
/// it refuses and names the agent, and the state that records the member is
/// kept. A listing that fails is "unknown", and unknown refuses.
pub(crate) fn unplaced_refusal(ctx: &Ctx<'_>, team: &TeamName, member: &Member) -> Option<String> {
    if member.place.is_some() || member.state == MemberState::Removed {
        return None;
    }
    let role = &member.role;
    let address = Address::of(team, role);
    let (_, host) = match ctx.adapters(&teamwire_store::Party::Member(role.clone())) {
        Ok(pair) => pair,
        Err(e) => {
            return Some(format!(
                "cannot tell whether {role}'s agent {address} is running ({e}); refusing \
                 rather than guess"
            ));
        }
    };
    match host.live_agent_names(HOST_TIMEOUT) {
        Ok(names) if names.iter().any(|n| n == address.as_str()) => Some(format!(
            "{role} has no recorded place, but its agent {address} is live in the host: a \
             launch was interrupted after the host started it. Stop that agent by hand \
             (close its pane or window), then run close again"
        )),
        Ok(_) => None,
        Err(e) => Some(format!(
            "cannot tell whether {role}'s agent {address} is running ({e}); refusing rather \
             than guess"
        )),
    }
}

/// Every busy, unplaced and dirty refusal for `members`, busy first.
pub(crate) fn member_refusals(ctx: &Ctx<'_>, team: &TeamName, members: &[Member]) -> Vec<String> {
    let mut out: Vec<String> = live(members)
        .filter_map(|m| busy_refusal(ctx, m, false))
        .collect();
    out.extend(live(members).filter_map(|m| unplaced_refusal(ctx, team, m)));
    out.extend(live(members).filter_map(|m| dirty_refusal(ctx, m)));
    out
}

/// The process that holds the close lock: its pid, with its start time when
/// one was recorded.
pub(crate) fn close_owner(team: &TeamRow) -> Option<ProcessRef> {
    team.closing_pid.map(|pid| ProcessRef {
        pid,
        started: team.closing_started,
    })
}

/// Why no close may start now, from the team row alone. An owner whose pid
/// now belongs to a process that started at another time is gone.
pub(crate) fn lock_refusal(ctx: &Ctx<'_>, team: &TeamRow) -> Option<String> {
    match team.state {
        TeamState::Open => None,
        TeamState::Closed => None,
        TeamState::Closing => Some(match close_owner(team) {
            Some(owner) if ctx.platform.process_alive(owner) => format!(
                "a close is already running (pid {}); wait for it",
                owner.pid
            ),
            Some(owner) => format!(
                "a close left its lock (pid {} is gone). Clear it with `teamwire close \
                 {} --clear-stale-lock`, then run the close again",
                owner.pid, team.name
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
