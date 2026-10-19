//! What close and removal must know about a member before they touch it:
//! whether it is busy, whether its worktree holds uncommitted work, and
//! whether the team's close lock is held. Each answer is a refusal text, so
//! the plan, the first check and every re-check under the lock say the same
//! thing (CLS-3).

use std::time::Duration;

use teamwire_core::platform::ProcessRef;
use teamwire_core::{
    Address, Harness, HostErrorCode, Liveness, Member, MemberState, Mode, PlaceState, Role,
    TeamName, TeamState,
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
    /// Neither the host nor the harness could say. Close refuses rather than
    /// guess, unless `--stop` accepts that on a single removal.
    Unknown(String),
}

/// The members that still count: a removed member was taken down earlier.
pub(crate) fn live(members: &[Member]) -> impl Iterator<Item = &Member> {
    members.iter().filter(|m| m.state != MemberState::Removed)
}

/// Asks the member's own host about the member's own place (CLS-7). A host
/// that reports no state (tmux) leaves the question to the member's harness.
pub(crate) fn busy_state(ctx: &Ctx<'_>, member: &Member) -> Busy {
    let Some(place) = &member.place else {
        return Busy::Idle;
    };
    let (harness, host) = match ctx.adapters(&teamwire_store::Party::Member(member.role.clone())) {
        Ok(pair) => pair,
        Err(e) => return Busy::Unknown(e.to_string()),
    };
    match host.state(place, HOST_TIMEOUT) {
        Ok(PlaceState::Idle | PlaceState::Gone) => Busy::Idle,
        Ok(PlaceState::Working) => Busy::Working,
        Ok(PlaceState::Blocked) => Busy::Blocked,
        Ok(PlaceState::Unknown) => harness_busy(harness, member),
        Err(e) if e.code == HostErrorCode::NotFound => Busy::Idle,
        Err(e) => Busy::Unknown(e.to_string()),
    }
}

/// The harness's answer for a member whose host cannot say: Claude's session
/// status. A session the harness cannot read, or reports dead while the place
/// is still there, is no answer.
fn harness_busy(harness: &dyn Harness, member: &Member) -> Busy {
    let unknown =
        || Busy::Unknown("the host reports no state and the harness cannot tell".to_owned());
    let Some(session) = &member.session_id else {
        return unknown();
    };
    match harness.is_alive(session, HOST_TIMEOUT) {
        Ok(Liveness::Alive(PlaceState::Idle)) => Busy::Idle,
        Ok(Liveness::Alive(PlaceState::Working)) => Busy::Working,
        Ok(Liveness::Alive(PlaceState::Blocked)) => Busy::Blocked,
        _ => unknown(),
    }
}

/// Why this member must not be closed now, if it is busy or unreadable.
/// `stop_allowed` is `--stop` on a single removal: a running turn, a dialog
/// or a state nobody can read is then accepted, and the removal says which.
pub(crate) fn busy_refusal(ctx: &Ctx<'_>, member: &Member, stop_allowed: bool) -> Option<String> {
    let role = &member.role;
    match busy_state(ctx, member) {
        Busy::Idle => None,
        Busy::Working if stop_allowed => None,
        Busy::Blocked if stop_allowed => None,
        Busy::Unknown(_) if stop_allowed => None,
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

/// How many names a rescue branch may take: the plain one, then `-2` to `-99`.
const RESCUE_NAMES: u32 = 99;

/// A worktree member's HEAD that is on no branch, as git reports it.
#[derive(Debug)]
pub(crate) struct Detached {
    pub(crate) sha: String,
    /// A branch, tag or remote ref already holds the commit.
    held: bool,
    /// `(sha, refname)` of every ref under `refs/heads/teamwire`, the only
    /// refs a rescue name can clash with.
    taken: Vec<(String, String)>,
    /// `teamwire/rescue/<team>-<role>-<sha7>`. Team names hold no hyphen,
    /// so the name splits one way.
    base: String,
}

/// The rescue branch a detached HEAD gets.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rescue {
    /// A rescue branch from an earlier close already points at the commit.
    Reuse(String),
    /// This name is free; close creates it.
    Create(String),
}

impl Rescue {
    pub(crate) fn into_name(self) -> String {
        match self {
            Self::Reuse(name) | Self::Create(name) => name,
        }
    }
}

impl Detached {
    pub(crate) fn short(&self) -> String {
        self.sha.chars().take(7).collect()
    }

    /// The rescue branch to use, skipping the names in `refused` (git turned
    /// them down). `None` when another ref already holds the commit. A name
    /// that exists at another commit is skipped, never moved.
    pub(crate) fn pick(&self, refused: &[String]) -> Result<Option<Rescue>, String> {
        let names = || {
            std::iter::once(self.base.clone())
                .chain((2..=RESCUE_NAMES).map(|n| format!("{}-{n}", self.base)))
        };
        let branches = || {
            self.taken
                .iter()
                .map(|(sha, r)| (sha, r.strip_prefix("refs/heads/").unwrap_or(r)))
        };
        if let Some(name) = names().find(|n| branches().any(|(sha, b)| b == n && *sha == self.sha))
        {
            return Ok(Some(Rescue::Reuse(name)));
        }
        if self.held {
            return Ok(None);
        }
        // git keeps refs as paths: `teamwire` or `teamwire/rescue` as a branch
        // blocks every name below it, and a name cannot also be a folder.
        let clash = |name: &str| {
            branches().any(|(_, b)| {
                b == name
                    || name.starts_with(&format!("{b}/"))
                    || b.starts_with(&format!("{name}/"))
            })
        };
        match names().find(|n| !refused.contains(n) && !clash(n)) {
            Some(name) => Ok(Some(Rescue::Create(name))),
            None => Err(format!(
                "HEAD is detached at {} and its commits are on no branch, and no rescue \
                 branch name is free ({} and -2 to -{RESCUE_NAMES} exist at other commits or \
                 git refuses them). Create a branch at {} first; close never moves a branch",
                self.short(),
                self.base,
                self.short()
            )),
        }
    }
}

/// The member's HEAD, when it is detached in a worktree that still exists.
pub(crate) fn detached(
    ctx: &Ctx<'_>,
    team: &TeamName,
    member: &Member,
) -> Result<Option<Detached>, String> {
    if member.mode != Mode::Worktree || !member.cwd.is_dir() {
        return Ok(None);
    }
    let cwd = &member.cwd;
    let out = git::run(ctx.platform, cwd, &["symbolic-ref", "-q", "HEAD"])?;
    if out.success() {
        return Ok(None);
    }
    if out.timed_out || out.exit_code != 1 {
        return Err(format!(
            "git symbolic-ref HEAD in {}: {}",
            cwd.display(),
            git::failure_text(&out)
        ));
    }
    let sha = git::stdout_of(ctx.platform, cwd, &["rev-parse", "HEAD"])?;
    let held = !git::stdout_of(
        ctx.platform,
        cwd,
        &[
            "for-each-ref",
            "--count=1",
            "--contains",
            &sha,
            "--format=%(refname)",
            "refs/heads",
            "refs/tags",
            "refs/remotes",
        ],
    )?
    .is_empty();
    let taken = git::stdout_of(
        ctx.platform,
        cwd,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/heads/teamwire",
        ],
    )?
    .lines()
    .filter_map(|l| l.split_once(' '))
    .map(|(sha, r)| (sha.to_owned(), r.to_owned()))
    .collect();
    let base = format!(
        "teamwire/rescue/{team}-{}-{}",
        member.role,
        sha.chars().take(7).collect::<String>()
    );
    Ok(Some(Detached {
        sha,
        held,
        taken,
        base,
    }))
}

/// The member's detached HEAD, read with the team name from the store.
/// `Ok(None)` also for a store with no team row: it records no worktree.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn detached_now(ctx: &Ctx<'_>, member: &Member) -> Result<Option<Detached>, String> {
    match ctx.store.read(|tx| tx.team()) {
        Ok(Some(team)) => detached(ctx, &team.name, member),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn cannot_tell(member: &Member, e: &str) -> String {
    format!(
        "cannot tell whether {}'s commits are on a branch ({e}); refusing rather than guess",
        member.role
    )
}

/// Why close cannot keep this member's detached commits on a branch, or
/// cannot tell whether it must.
fn rescue_refusal(ctx: &Ctx<'_>, member: &Member) -> Option<String> {
    match detached_now(ctx, member) {
        Ok(Some(head)) => head
            .pick(&[])
            .err()
            .map(|e| format!("{}'s {e}", member.role)),
        Ok(None) => None,
        Err(e) => Some(cannot_tell(member, &e)),
    }
}

/// Why a worktree must not be removed this moment: its HEAD is detached and
/// no ref holds the commit. Close makes the rescue branch first, so this
/// means the agent committed after that, while it still ran; removing the
/// worktree would leave the new commit on no ref. Asked right before
/// `git worktree remove`; a rerun gives the new commit its own rescue branch.
pub(crate) fn stranded_refusal(ctx: &Ctx<'_>, member: &Member) -> Option<String> {
    match detached_now(ctx, member) {
        Ok(Some(head)) if !head.held => Some(format!(
            "{}'s HEAD moved to {} after its rescue branch was made, and no ref holds it; the \
             worktree is kept. Run close again to keep that commit on a branch",
            member.role,
            head.short()
        )),
        Ok(_) => None,
        Err(e) => Some(cannot_tell(member, &e)),
    }
}

/// Why this member's checkout must not be removed now: it has uncommitted
/// work, its detached commits can get no rescue branch, or git cannot say.
pub(crate) fn dirty_refusal(ctx: &Ctx<'_>, member: &Member) -> Option<String> {
    match dirty_paths(ctx, member) {
        Ok(paths) if paths.is_empty() => rescue_refusal(ctx, member),
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
