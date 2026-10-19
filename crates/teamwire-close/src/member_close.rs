//! The per-member steps of a close: keep the member's branches (a detached
//! HEAD's commits get a rescue branch first), take the worktree back, close
//! the place, and say what a kept branch has that no remote has.

use std::time::Duration;

use teamwire_core::{Address, HostErrorCode, Member, MemberState, Mode, RunOutput, TeamName};
use teamwire_store::{Ctx, Party};

use crate::facts::{self, Detached, Rescue};
use crate::git;

const HOST_TIMEOUT: Duration = Duration::from_secs(30);

/// `git worktree remove` for one member, without force; the branch stays.
pub(crate) fn remove_worktree(ctx: &Ctx<'_>, member: &Member) -> Result<String, String> {
    let role = &member.role;
    let Some(repo) = &member.repo else {
        return Ok(format!(
            "{role} has no repository recorded; nothing to remove"
        ));
    };
    if !member.cwd.is_dir() {
        return Ok(format!(
            "{role}'s worktree {} was already gone",
            member.cwd.display()
        ));
    }
    if let Some(why) = facts::stranded_refusal(ctx, member) {
        return Err(why);
    }
    let path = member.cwd.display().to_string();
    let out = git::run(ctx.platform, repo, &["worktree", "remove", &path])
        .map_err(|e| format!("{role}: {e}"))?;
    if out.success() {
        Ok(format!("{role}'s worktree {path} removed"))
    } else {
        Err(format!(
            "{role}: git worktree remove: {}",
            git::failure_text(&out)
        ))
    }
}

/// How many ignored entries the warning names before it counts the rest.
const IGNORED_SHOWN: usize = 10;

/// Top-level build folders every checkout regenerates. Naming them would put
/// a warning on nearly every close, and people learn to skip such a line.
const BUILD_DIRS: [&str; 2] = ["target/", "node_modules/"];

/// The warning for a worktree whose removal deletes gitignored files, such
/// as `.env.local`: `git worktree remove` keeps no copy of them, and the
/// dirty check does not list them. `TASK.md` is the brief's own copy, which
/// launch excludes and the dirty check ignores, so it is not named. Nor are the top-level `BUILD_DIRS`. `None` when there are none, or when git
/// cannot say (the dirty check has already refused in that case).
pub(crate) fn ignored_warning(ctx: &Ctx<'_>, member: &Member) -> Option<String> {
    if member.mode != Mode::Worktree || !member.cwd.is_dir() {
        return None;
    }
    let out = git::run(
        ctx.platform,
        &member.cwd,
        &["status", "--porcelain", "--ignored"],
    )
    .ok()
    .filter(RunOutput::success)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let ignored: Vec<&str> = stdout
        .lines()
        .filter(|l| !teamwire_verify::is_task_file_status(l))
        .filter_map(|l| l.strip_prefix("!! "))
        .filter(|p| !BUILD_DIRS.contains(p))
        .collect();
    if ignored.is_empty() {
        return None;
    }
    let mut named: Vec<String> = ignored
        .iter()
        .take(IGNORED_SHOWN)
        .map(|p| (*p).to_owned())
        .collect();
    if ignored.len() > IGNORED_SHOWN {
        named.push(format!("and {} more", ignored.len() - IGNORED_SHOWN));
    }
    let named = named.join(", ");
    Some(format!("removal deletes ignored files: {named}"))
}

/// Closes the member's own place through the member's own host (CLS-7).
/// A place the host no longer knows, or one that now holds another agent
/// (a reused id), is closed already.
pub(crate) fn close_place(
    ctx: &Ctx<'_>,
    team: &TeamName,
    member: &Member,
) -> Result<Option<String>, String> {
    let Some(place) = &member.place else {
        return Ok(None);
    };
    let role = &member.role;
    let (_, host) = ctx
        .adapters(&Party::Member(role.clone()))
        .map_err(|e| format!("{role}: {e}"))?;
    match host.close(place, &Address::of(team, role), HOST_TIMEOUT) {
        Ok(()) => Ok(Some(format!("{role}'s place {} closed", place.id))),
        Err(e) if e.code == HostErrorCode::NotFound => Ok(Some(format!(
            "{role}'s place {} was already gone",
            place.id
        ))),
        Err(e) => Err(format!("{role}: closing place {}: {e}", place.id)),
    }
}

/// The branches close keeps for one member, and what they hold that no
/// remote has.
#[derive(Debug, Default)]
pub(crate) struct Kept {
    pub(crate) branches: Vec<String>,
    pub(crate) unpushed: Vec<String>,
}

/// The branches close keeps for one member: the recorded one, unless the
/// user deleted it, and a rescue branch for a detached HEAD whose commits no
/// branch, tag or remote ref holds. Removing that worktree would leave those
/// commits on no ref, so the rescue branch is made here, before the worktree
/// goes; `dry_run` only names it. An error means the commits could not be
/// kept, and the caller must not remove the worktree.
pub(crate) fn kept_branches(
    ctx: &Ctx<'_>,
    team: &TeamName,
    member: &Member,
    dry_run: bool,
) -> Result<Kept, String> {
    let role = &member.role;
    let mut kept = Kept::default();
    kept.branches.extend(member.branch.clone().filter(|b| {
        !member
            .repo
            .as_deref()
            .is_some_and(|r| git::branch_is_gone(ctx.platform, r, b))
    }));
    let rescue = match facts::detached(ctx, team, member).map_err(|e| format!("{role}: {e}"))? {
        Some(head) if dry_run => head
            .pick(&[])
            .map_err(|e| format!("{role}'s {e}"))?
            .map(Rescue::into_name),
        Some(head) => rescue(ctx, member, &head)?,
        None => None,
    };
    kept.branches.extend(rescue.clone());
    if !kept.branches.is_empty() {
        kept.unpushed = unpushed(ctx, member, rescue.as_deref());
    }
    Ok(kept)
}

/// Creates (or reuses) the rescue branch at the detached HEAD. `git branch`
/// runs without `-f`, so an existing branch never moves; a name git refuses
/// sends close to the next one.
fn rescue(ctx: &Ctx<'_>, member: &Member, head: &Detached) -> Result<Option<String>, String> {
    let role = &member.role;
    let mut refused = Vec::new();
    let mut said = String::new();
    loop {
        let name = match head.pick(&refused) {
            Ok(Some(Rescue::Create(name))) => name,
            Ok(other) => return Ok(other.map(Rescue::into_name)),
            Err(e) if said.is_empty() => return Err(format!("{role}'s {e}")),
            Err(e) => return Err(format!("{role}'s {e}; git said: {said}")),
        };
        let out = git::run(ctx.platform, &member.cwd, &["branch", &name, &head.sha])
            .map_err(|e| format!("{role}: {e}"))?;
        if out.success() {
            return Ok(Some(name));
        }
        said = git::failure_text(&out);
        refused.push(name);
    }
}

/// What a kept branch has that no remote has (CLS-8): commits ahead of its
/// upstream, or, with no upstream, every commit since the member started.
/// The note names the branch HEAD is on, which need not be the recorded one;
/// a detached HEAD is named by its rescue branch.
fn unpushed(ctx: &Ctx<'_>, member: &Member, rescue: Option<&str>) -> Vec<String> {
    if member.state == MemberState::Failed || !member.cwd.is_dir() {
        return Vec::new();
    }
    let on = git::stdout_of(
        ctx.platform,
        &member.cwd,
        &["symbolic-ref", "--short", "-q", "HEAD"],
    )
    .ok()
    .or_else(|| rescue.map(str::to_owned));
    let branch = on.map_or_else(|| "the detached HEAD".to_owned(), |b| format!("branch {b}"));
    let count = |args: &[&str]| -> Option<u64> {
        let out = git::run(ctx.platform, &member.cwd, args).ok()?;
        out.success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().parse().ok())
            .flatten()
    };
    let role = &member.role;
    if let Some(ahead) = count(&["rev-list", "--count", "@{upstream}..HEAD"]) {
        return if ahead > 0 {
            vec![format!(
                "{role}: {branch} is {ahead} commit(s) ahead of its upstream, not pushed"
            )]
        } else {
            Vec::new()
        };
    }
    let since = member
        .base_sha
        .as_ref()
        .map(|sha| format!("{sha}..HEAD"))
        .unwrap_or_else(|| "HEAD".to_owned());
    match count(&["rev-list", "--count", &since]) {
        Some(n) if n > 0 => vec![format!(
            "{role}: {branch} has no upstream; {n} commit(s) exist only on this machine"
        )],
        _ => Vec::new(),
    }
}
