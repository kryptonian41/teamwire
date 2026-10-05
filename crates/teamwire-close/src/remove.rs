//! Taking one member out of a live team (CLS-4, CLS-5, CLS-6): the same
//! checks as a whole close for this member, and two more that protect work
//! nobody has looked at yet. The member's row stays, marked removed.

use serde_json::json;
use teamwire_core::{HostErrorCode, Member, MemberKind, MemberState, Mode, Role, TeamState};
use teamwire_store::{Ctx, Party, TeamRow};

use crate::facts::{self, Busy, busy_refusal, busy_state, dirty_refusal, unplaced_refusal};
use crate::{CloseOpts, CloseReport, StepOutcome, git, team_close};

/// What stands between this member and removal.
#[derive(Debug, Default)]
pub(crate) struct Gate {
    /// Rules that refuse the removal.
    pub(crate) refusals: Vec<String>,
    /// CLS-5 reasons that `--unverified` set aside; each is printed.
    pub(crate) overridden: Vec<String>,
}

/// Busy, dirty, and the two checks of CLS-5, judged with the flags given.
pub(crate) fn gate(
    ctx: &Ctx<'_>,
    team: &TeamRow,
    members: &[Member],
    member: &Member,
    opts: &CloseOpts,
) -> Gate {
    let mut gate = Gate::default();
    gate.refusals.extend(busy_refusal(ctx, member, opts.stop));
    gate.refusals
        .extend(unplaced_refusal(ctx, &team.name, member));
    gate.refusals.extend(dirty_refusal(ctx, member));
    let unchecked = unchecked(ctx, team, members, member);
    if opts.unverified {
        gate.overridden = unchecked;
    } else if !unchecked.is_empty() {
        gate.refusals.extend(unchecked);
        gate.refusals
            .push("pass --unverified only to replace a member that went the wrong way".to_owned());
    }
    gate
}

/// Why removing this member now would lose a chance to check or fix its work
/// (CLS-5). Empty when nothing would be lost.
fn unchecked(ctx: &Ctx<'_>, team: &TeamRow, members: &[Member], member: &Member) -> Vec<String> {
    let mut why = Vec::new();
    let role = &member.role;
    // A member that never started has no work to check.
    if member.kind == MemberKind::Implementer && member.state == MemberState::Started {
        why.extend(verify_gap(ctx, team, member));
    }
    for closer in members {
        if closer.kind == MemberKind::Closer
            && closer.state == MemberState::Started
            && closer.role != member.role
            && !reported(ctx, closer)
        {
            why.push(format!(
                "closer {} has not reported yet. Its findings may name {role}, and the owner \
                 fixes them when the lead reprompts it",
                closer.role
            ));
        }
    }
    why
}

/// The closer's newest completion is for its current generation.
fn reported(ctx: &Ctx<'_>, closer: &Member) -> bool {
    ctx.store
        .read(|tx| tx.latest_completion(&closer.role))
        .ok()
        .flatten()
        .is_some_and(|c| c.completion.generation >= closer.generation)
}

/// A passing verify record for the member's HEAD as it is now, or why not.
/// A record, and a waiver, made for an older HEAD do not count (VER-5).
fn verify_gap(ctx: &Ctx<'_>, team: &TeamRow, member: &Member) -> Option<String> {
    let role = &member.role;
    let rerun = format!(
        "teamwire verify skips removed members, so its work would never be checked. Run: \
         teamwire verify {} --role {role}",
        team.name
    );
    let head = match git::stdout_of(ctx.platform, &member.cwd, &["rev-parse", "HEAD"]) {
        Ok(head) => head,
        Err(e) => return Some(format!("{role}'s HEAD cannot be read ({e}); {rerun}")),
    };
    let records = match ctx
        .store
        .read(|tx| tx.verify_records(role)?.all_or_corrupt())
    {
        Ok(records) => records,
        Err(e) => return Some(format!("{role}'s verify records cannot be read ({e})")),
    };
    let short = |sha: &str| sha.chars().take(7).collect::<String>();
    let Some(newest) = records.last() else {
        return Some(format!("{role} has no verify record. {rerun}"));
    };
    match records.iter().rfind(|r| r.record.head_sha == head) {
        None => Some(format!(
            "{role} was verified at {} but HEAD is now {}: a record or a waiver for an older \
             HEAD does not carry over. {rerun}",
            short(&newest.record.head_sha),
            short(&head)
        )),
        Some(row) if row.record.verdict == "PASS" => None,
        Some(row) => Some(format!(
            "{role}'s verify at HEAD {} is {}, not PASS. Fix what it flagged, or waive each \
             flag with `teamwire verify {} --role {role} --waive FLAG --reason TEXT`, then \
             verify again",
            short(&head),
            row.record.verdict,
            team.name
        )),
    }
}

fn step(name: &str, ok: bool, detail: impl Into<String>) -> StepOutcome {
    StepOutcome {
        step: name.to_owned(),
        ok,
        detail: detail.into(),
    }
}

/// Removes one member. The plan of `plan_close_with` had no refusal.
pub(crate) fn run(
    ctx: &mut Ctx<'_>,
    role: &Role,
    opts: &CloseOpts,
    observer: &dyn Fn(&str),
) -> CloseReport {
    let mut report = CloseReport::default();
    let (team, members) = match team_close::read(ctx) {
        Ok(pair) => pair,
        Err(why) => {
            report.refusals.push(why);
            return report;
        }
    };
    let Some(member) = members.iter().find(|m| &m.role == role) else {
        report.refusals.push(facts::no_such_member(role, &team));
        return report;
    };
    if member.state == MemberState::Removed {
        report.steps.push(step(
            "mark_removed",
            true,
            format!("{role} was already removed"),
        ));
        return report;
    }
    if team.state != TeamState::Open {
        report.refusals.push(format!(
            "team {} is {}: a member cannot be removed now",
            team.name, team.state
        ));
        return report;
    }
    let gate = gate(ctx, &team, &members, member, opts);
    if !gate.refusals.is_empty() {
        report.refusals = gate.refusals;
        return report;
    }
    for why in &gate.overridden {
        report.steps.push(step(
            "unverified",
            true,
            format!("removing anyway (--unverified): {why}"),
        ));
    }
    let stopped_mid_turn = matches!(busy_state(ctx, member), Busy::Working | Busy::Blocked);
    if stopped_mid_turn {
        report.steps.push(step(
            "stop",
            true,
            format!("stopping {role} mid-turn (--stop); that turn writes no report"),
        ));
    }
    if let Some(branch) = &member.branch {
        report.branches_kept.push(branch.clone());
        report
            .unpushed
            .extend(team_close::unpushed(ctx, member, branch));
    }
    let member = member.clone();
    // The lock is read inside a write transaction, so a close that took it
    // after the first read above cannot be missed (CLS-3).
    if let Err(why) = ensure_open(ctx) {
        report.refusals.push(why);
        return report;
    }
    let mut go = |name: &str,
                  report: &mut CloseReport,
                  work: &dyn Fn(&mut Ctx<'_>) -> Result<String, String>|
     -> bool {
        observer(name);
        let result = work(ctx);
        let ok = result.is_ok();
        let (Ok(detail) | Err(detail)) = result;
        report.steps.push(step(name, ok, detail));
        if !ok {
            report.stopped_at = Some(name.to_owned());
        }
        ok
    };
    // The place first, so a stopped agent cannot write into the checkout as
    // it is taken away; then the checkout, which git refuses to remove over
    // uncommitted work.
    if go("close_place", &mut report, &|c| {
        guard(c, &member, opts.stop)?;
        close_place(c, &member)
    }) && go("remove_worktree", &mut report, &|c| {
        guard(c, &member, opts.stop)?;
        remove_worktree(c, &member)
    }) {
        go("mark_removed", &mut report, &|c| {
            mark_removed(c, &member, &gate.overridden, stopped_mid_turn)
        });
    }
    report
}

/// Busy and dirty state read again right before a destructive step. A
/// `--stop` stays allowed: the caller already accepted the lost turn.
fn guard(ctx: &Ctx<'_>, member: &Member, stop: bool) -> Result<(), String> {
    if let Ok(Some(team)) = ctx.store.read(|tx| tx.team())
        && team.state != TeamState::Open
    {
        return Err(format!(
            "the world changed: team {} is {}: a member cannot be removed now",
            team.name, team.state
        ));
    }
    match busy_refusal(ctx, member, stop).or_else(|| dirty_refusal(ctx, member)) {
        Some(why) => Err(format!("the world changed: {why}")),
        None => Ok(()),
    }
}

fn close_place(ctx: &Ctx<'_>, member: &Member) -> Result<String, String> {
    let role = &member.role;
    let Some(place) = &member.place else {
        return Ok(format!("{role} has no place recorded"));
    };
    let (_, host) = ctx
        .adapters(&Party::Member(role.clone()))
        .map_err(|e| format!("{role}: {e}"))?;
    match host.close(place, std::time::Duration::from_secs(30)) {
        Ok(()) => Ok(format!("{role}'s place {} closed", place.id)),
        Err(e) if e.code == HostErrorCode::NotFound => {
            Ok(format!("{role}'s place {} was already gone", place.id))
        }
        Err(e) => Err(format!("{role}: closing place {}: {e}", place.id)),
    }
}

fn remove_worktree(ctx: &Ctx<'_>, member: &Member) -> Result<String, String> {
    let role = &member.role;
    let (Mode::Worktree, Some(repo)) = (member.mode, &member.repo) else {
        return Ok(format!("{role} has no worktree"));
    };
    if !member.cwd.is_dir() {
        return Ok(format!(
            "{role}'s worktree {} was already gone",
            member.cwd.display()
        ));
    }
    let path = member.cwd.display().to_string();
    let out = git::run(ctx.platform, repo, &["worktree", "remove", &path])
        .map_err(|e| format!("{role}: {e}"))?;
    if !out.success() {
        return Err(format!(
            "{role}: git worktree remove: {}",
            git::failure_text(&out)
        ));
    }
    git::run(ctx.platform, repo, &["worktree", "prune"]).ok();
    Ok(format!(
        "{role}'s worktree {path} removed{}",
        member
            .branch
            .as_ref()
            .map_or(String::new(), |b| format!("; branch {b} kept"))
    ))
}

/// The row stays, with `removed_at` and a note, so the role is never reused
/// and later steps skip it (CLS-6).
fn mark_removed(
    ctx: &mut Ctx<'_>,
    member: &Member,
    overridden: &[String],
    stopped: bool,
) -> Result<String, String> {
    let role = &member.role;
    let now = ctx.clock.now();
    let note = if stopped {
        "stopped while busy"
    } else {
        "removed"
    };
    let data = json!({
        "stopped": stopped,
        "unverified": !overridden.is_empty(),
        "unverified_reasons": overridden,
    });
    ctx.store
        .write(|tx| {
            open_or_constraint(tx)?;
            tx.remove_member(now, role, Some(note), &data)
        })
        .map(|_| format!("{role} marked removed ({note})"))
        .map_err(|e| format!("{role} could not be marked removed: {e}"))
}

/// Inside a write transaction: the team is `open`, or the write fails.
fn open_or_constraint(tx: &teamwire_store::Tx<'_>) -> Result<(), teamwire_store::StoreError> {
    match tx.reader().team()? {
        Some(team) if team.state != TeamState::Open => {
            Err(teamwire_store::StoreError::Constraint(format!(
                "team {} is {}: a member cannot be removed now",
                team.name, team.state
            )))
        }
        _ => Ok(()),
    }
}

/// The refusal text when a close took the lock since the plan was read.
fn ensure_open(ctx: &mut Ctx<'_>) -> Result<(), String> {
    ctx.store
        .write(open_or_constraint)
        .map_err(|e| e.to_string())
}
