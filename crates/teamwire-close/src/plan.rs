//! The plan: what a close would do, or why it would refuse (`--dry-run`).
//! It reads the store, the hosts and git, and changes nothing.

use teamwire_core::{Member, Role, TeamState};
use teamwire_store::{Ctx, TeamRow};

use crate::facts::{self, live};
use crate::member_close::{ignored_warning, kept_branches};
use crate::{CloseOpts, ClosePlan, CloseStep, CloseTarget, archive, delete, remove};

fn step(name: &str, detail: impl Into<String>) -> CloseStep {
    CloseStep {
        step: name.to_owned(),
        detail: detail.into(),
    }
}

/// The plan for `target`, judged with the flags the close will run with.
pub fn plan_close_with(ctx: &Ctx<'_>, target: CloseTarget, opts: &CloseOpts) -> ClosePlan {
    if target == CloseTarget::Team
        && let Some((team, dir)) = crate::empty::nothing_recorded(ctx, opts)
    {
        return ClosePlan {
            steps: vec![crate::empty::plan(&team, &dir)],
            refusals: Vec::new(),
        };
    }
    let (team, members) = match read(ctx) {
        Ok(pair) => pair,
        Err(why) => {
            return ClosePlan {
                steps: Vec::new(),
                refusals: vec![why],
            };
        }
    };
    match target {
        CloseTarget::Team => team_plan(ctx, &team, &members, opts),
        CloseTarget::Member(role) => member_plan(ctx, &team, &members, &role, opts),
    }
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn read(ctx: &Ctx<'_>) -> Result<(TeamRow, Vec<Member>), String> {
    let team = ctx
        .store
        .read(|tx| tx.team())
        .map_err(|e| format!("the team cannot be read: {e}"))?
        .ok_or_else(|| "the store has no team row".to_owned())?;
    let members = ctx
        .store
        .read(|tx| tx.members()?.all_or_corrupt())
        .map_err(|e| format!("the members cannot be read: {e}"))?;
    Ok((team, members))
}

fn team_plan(ctx: &Ctx<'_>, team: &TeamRow, members: &[Member], opts: &CloseOpts) -> ClosePlan {
    let mut plan = ClosePlan::default();
    if team.state == TeamState::Closed && !ctx.home.join("teams").join(team.name.as_str()).is_dir()
    {
        plan.refusals
            .push(format!("team {} is already closed", team.name));
        return plan;
    }
    if let Some(why) = facts::lock_refusal(ctx, team) {
        plan.refusals.push(why);
        return plan;
    }
    if team.state == TeamState::Open {
        plan.refusals
            .extend(facts::member_refusals(ctx, &team.name, members));
        plan.refusals.extend(crate::afk::report_refusal(ctx, team));
        let checked = live(members).count();
        plan.steps.push(step(
            "check",
            if plan.refusals.is_empty() {
                format!("{checked} member(s) checked: all idle, no uncommitted work")
            } else {
                format!(
                    "{checked} member(s) checked, {} refusal(s)",
                    plan.refusals.len()
                )
            },
        ));
        plan.steps.push(step(
            "lock",
            "take the close lock: launch, reprompt and removal wait",
        ));
    }
    let dest = archive::destination(ctx, &team.name, opts);
    plan.steps.push(step(
        "archive",
        format!(
            "back up team.db, copy the other files, write SHA256SUMS, verify by content: {}",
            dest.display()
        ),
    ));
    if team.state == TeamState::Open {
        plan.steps
            .push(step("remove_worktrees", worktree_list(ctx, team, members)));
        plan.steps.extend(ignored_steps(ctx, members));
        plan.steps.push(step("close_places", place_list(members)));
        plan.steps.push(step("release_hold", hold_detail(team)));
        plan.steps
            .push(step("finish", "mark the team closed (team.closed)"));
    }
    plan.steps.push(step(
        "delete_state",
        if opts.keep_state {
            "keep the team directory, marked closed".to_owned()
        } else {
            let teams = ctx.home.join("teams").join(team.name.as_str());
            let cache = delete::closer_cache(ctx, &team.name);
            if cache.symlink_metadata().is_ok() {
                format!("delete {} and {}", teams.display(), cache.display())
            } else {
                format!("delete {}", teams.display())
            }
        },
    ));
    // An error here is already a refusal from the dirty check.
    let kept: Vec<String> = live(members)
        .filter(|m| m.mode == teamwire_core::Mode::Worktree)
        .filter_map(|m| kept_branches(ctx, &team.name, m, true).ok())
        .flat_map(|k| k.branches)
        .collect();
    if !kept.is_empty() {
        plan.steps.push(step("branches_kept", kept.join(", ")));
    }
    plan
}

/// One step per worktree whose removal deletes ignored files, named by role
/// so the line reads `<role>: removal deletes ignored files: ...`.
fn ignored_steps<'a>(
    ctx: &'a Ctx<'_>,
    members: &'a [Member],
) -> impl Iterator<Item = CloseStep> + 'a {
    live(members).filter_map(|m| ignored_warning(ctx, m).map(|d| step(m.role.as_str(), d)))
}

/// The worktrees, and each member whose place closes right before its
/// worktree goes because its HEAD is detached.
fn worktree_list(ctx: &Ctx<'_>, team: &TeamRow, members: &[Member]) -> String {
    let worktrees: Vec<&Member> = live(members)
        .filter(|m| m.mode == teamwire_core::Mode::Worktree)
        .collect();
    if worktrees.is_empty() {
        return "no worktrees".to_owned();
    }
    let paths: Vec<String> = worktrees
        .iter()
        .map(|m| format!("{} ({})", m.role, m.cwd.display()))
        .collect();
    let mut parts = vec![format!(
        "git worktree remove, without force: {}",
        paths.join(", ")
    )];
    parts.extend(
        worktrees
            .iter()
            .filter(|m| matches!(facts::detached(ctx, &team.name, m), Ok(Some(_))))
            .map(|m| {
                format!(
                    "{}'s HEAD is detached, so its place is closed right before its worktree \
                     is removed",
                    m.role
                )
            }),
    );
    parts.join("; ")
}

fn place_list(members: &[Member]) -> String {
    let places: Vec<String> = live(members)
        .filter_map(|m| m.place.as_ref().map(|p| format!("{} ({})", m.role, p.id)))
        .collect();
    if places.is_empty() {
        "no places recorded".to_owned()
    } else {
        format!("close each member's own place: {}", places.join(", "))
    }
}

fn hold_detail(team: &TeamRow) -> String {
    match team.sleep_hold {
        Some(hold) if hold.owned => format!(
            "release the sleep hold (pid {}) unless another open team still records it",
            hold.pid
        ),
        Some(hold) => format!(
            "leave the sleep hold (pid {}) running: teamwire did not start it",
            hold.pid
        ),
        None => "no sleep hold is recorded".to_owned(),
    }
}

fn member_plan(
    ctx: &Ctx<'_>,
    team: &TeamRow,
    members: &[Member],
    role: &Role,
    opts: &CloseOpts,
) -> ClosePlan {
    let mut plan = ClosePlan::default();
    let Some(member) = members.iter().find(|m| &m.role == role) else {
        plan.refusals.push(facts::no_such_member(role, team));
        return plan;
    };
    if member.state == teamwire_core::MemberState::Removed {
        plan.steps
            .push(step("mark_removed", format!("{role} was already removed")));
        return plan;
    }
    if team.state != TeamState::Open {
        plan.refusals.push(format!(
            "team {} is {}: a member cannot be removed now",
            team.name, team.state
        ));
        return plan;
    }
    let gate = remove::gate(ctx, team, members, member, opts);
    plan.refusals = gate.refusals;
    plan.steps.extend(gate.overridden.into_iter().map(|why| {
        step(
            "unverified",
            format!("removing anyway (--unverified): {why}"),
        )
    }));
    plan.steps
        .push(step("close_place", format!("close {role}'s own place")));
    // A pane member shares a directory it does not own; only a worktree goes.
    if member.mode == teamwire_core::Mode::Worktree {
        plan.steps.push(step(
            "remove_worktree",
            format!(
                "git worktree remove, without force: {}",
                member.cwd.display()
            ),
        ));
    }
    plan.steps
        .extend(ignored_warning(ctx, member).map(|d| step(role.as_str(), d)));
    plan.steps.push(step(
        "mark_removed",
        format!("keep {role}'s row, marked removed"),
    ));
    if let Ok(kept) = kept_branches(ctx, &team.name, member, true)
        && !kept.branches.is_empty()
    {
        plan.steps
            .push(step("branches_kept", kept.branches.join(", ")));
    }
    plan
}
