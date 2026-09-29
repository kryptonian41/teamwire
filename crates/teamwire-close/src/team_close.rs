//! Closing the whole team, in the fixed order of CLS-1, under the close lock.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;
use teamwire_core::{EventType, HostErrorCode, Member, MemberState, Mode, Pid, TeamState};
use teamwire_store::{CloseEnd, Ctx, Party, TeamRow};

use crate::facts::{self, live};
use crate::{CloseOpts, CloseReport, StepOutcome, archive, git, hold};

const HOST_TIMEOUT: Duration = Duration::from_secs(30);

/// `close --clear-stale-lock`: clears a lock whose owner is dead, and nothing
/// else. A live owner is never cleared.
pub(crate) fn clear_stale_lock(ctx: &mut Ctx<'_>) -> CloseReport {
    let mut report = CloseReport::default();
    let team = match read(ctx) {
        Ok((team, _)) => team,
        Err(why) => {
            report.refusals.push(why);
            return report;
        }
    };
    let holder = team.closing_pid.filter(|pid| ctx.platform.pid_alive(*pid));
    if team.state == TeamState::Closing {
        if let Some(pid) = holder {
            report.refusals.push(format!(
                "the close lock belongs to pid {pid}, which is running; nothing was cleared"
            ));
            return report;
        }
        let now = ctx.clock.now();
        return match ctx.store.write(|tx| tx.clear_stale_close_lock(now)) {
            Ok(_) => {
                report.steps.push(StepOutcome {
                    step: "clear_stale_lock".to_owned(),
                    ok: true,
                    detail: "the close lock of a dead process was cleared; run the close again"
                        .to_owned(),
                });
                report
            }
            Err(e) => {
                report.stopped_at = Some("clear_stale_lock".to_owned());
                report.steps.push(StepOutcome {
                    step: "clear_stale_lock".to_owned(),
                    ok: false,
                    detail: e.to_string(),
                });
                report
            }
        };
    }
    report.steps.push(StepOutcome {
        step: "clear_stale_lock".to_owned(),
        ok: true,
        detail: "no close lock is held".to_owned(),
    });
    report
}

/// The team and its members, read fresh.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub(crate) fn read(ctx: &Ctx<'_>) -> Result<(TeamRow, Vec<Member>), String> {
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

/// Runs the close of a team whose plan had no refusal.
pub(crate) fn run(ctx: &mut Ctx<'_>, opts: CloseOpts, observer: &dyn Fn(&str)) -> CloseReport {
    let team = match read(ctx) {
        Ok((team, _)) => team,
        Err(why) => {
            return CloseReport {
                refusals: vec![why],
                ..CloseReport::default()
            };
        }
    };
    let mut seq = Seq {
        team_dir: ctx.home.join("teams").join(team.name.as_str()),
        ctx,
        observer,
        report: CloseReport::default(),
        opts,
    };
    if team.state == TeamState::Closed {
        seq.finish_resumed();
    } else {
        seq.run_locked();
    }
    seq.report
}

struct Seq<'a, 'c> {
    ctx: &'a mut Ctx<'c>,
    observer: &'a dyn Fn(&str),
    report: CloseReport,
    opts: CloseOpts,
    team_dir: PathBuf,
}

impl Seq<'_, '_> {
    /// Records how a step went, in the report and as `close.step`. Returns
    /// whether the sequence goes on. A failure names the step it stopped at.
    fn done(&mut self, step: &str, result: Result<String, String>) -> bool {
        let (ok, detail) = match result {
            Ok(detail) => (true, detail),
            Err(detail) => (false, detail),
        };
        let now = self.ctx.clock.now();
        let data = json!({
            "step": step,
            "outcome": if ok { "ok" } else { "failed" },
            "reason": detail,
        });
        if let Err(e) = self
            .ctx
            .store
            .write(|tx| tx.record_event(now, EventType::CloseStep, None, None, &data))
        {
            tracing::warn!(step, error = %e, "close.step could not be recorded");
        }
        self.report.steps.push(StepOutcome {
            step: step.to_owned(),
            ok,
            detail,
        });
        if !ok {
            self.report.stopped_at = Some(step.to_owned());
        }
        ok
    }

    /// The steps of a team that is `open`: take the lock, then the order.
    fn run_locked(&mut self) {
        (self.observer)("lock");
        let now = self.ctx.clock.now();
        let pid = Pid::new(std::process::id());
        match self.ctx.store.write(|tx| tx.begin_close(now, pid)) {
            Ok(true) => {}
            Ok(false) => {
                self.report
                    .refusals
                    .push("the team is not open: another close holds the lock".to_owned());
                return;
            }
            Err(e) => {
                self.done(
                    "lock",
                    Err(format!("the close lock could not be taken: {e}")),
                );
                return;
            }
        }
        self.done("lock", Ok(format!("close lock taken by pid {pid}")));
        if !self.locked_steps() {
            let step = self.report.stopped_at.clone().unwrap_or_default();
            self.give_up(&step);
            return;
        }
        // The team is closed. What is left must not reopen it: the archive
        // and the state directory are ordinary steps that can be run again.
        self.finish_resumed();
    }

    /// Undoes the lock after a step failed, so the next run starts from the
    /// first step. The archive that was taken stays, and is named in the report.
    fn give_up(&mut self, step: &str) {
        let now = self.ctx.clock.now();
        let end = CloseEnd::Failed {
            step: step.to_owned(),
        };
        if let Err(e) = self.ctx.store.write(|tx| tx.finish_close(now, &end)) {
            tracing::warn!(step, error = %e, "the close lock could not be given back");
        }
    }

    /// Archive, verify, remove worktrees, close places, release the hold, and
    /// mark the team closed. False at the first step that fails.
    fn locked_steps(&mut self) -> bool {
        self.archive()
            && self.verify_archive(false)
            && self.remove_worktrees()
            && self.close_places()
            && self.release_hold()
            && self.finish()
    }

    fn archive(&mut self) -> bool {
        (self.observer)("archive");
        let (team, _) = match read(self.ctx) {
            Ok(pair) => pair,
            Err(why) => return self.done("archive", Err(why)),
        };
        let dest = archive::destination(self.ctx, &team.name, &self.opts);
        let result = archive::write(self.ctx, &self.team_dir, &dest);
        if result.is_ok() || dest.exists() {
            self.report.archive = Some(dest);
        }
        self.done("archive", result)
    }

    fn verify_archive(&mut self, expect_closed: bool) -> bool {
        (self.observer)("verify_archive");
        let Some(dest) = self.report.archive.clone() else {
            return self.done(
                "verify_archive",
                Err("there is no archive to verify".to_owned()),
            );
        };
        let result = archive::verify(self.ctx, &self.team_dir, &dest, expect_closed)
            .map(|()| "every file hashes to SHA256SUMS and matches the team directory".to_owned());
        self.done("verify_archive", result)
    }

    /// Busy and dirty state, read again under the lock (CLS-3), right before
    /// a step that destroys something.
    fn recheck(&self) -> Result<Vec<Member>, String> {
        let (_, members) = read(self.ctx)?;
        let refusals = facts::member_refusals(self.ctx, &members);
        if refusals.is_empty() {
            Ok(members)
        } else {
            Err(format!(
                "the world changed under the lock: {}",
                refusals.join("; ")
            ))
        }
    }

    fn remove_worktrees(&mut self) -> bool {
        (self.observer)("remove_worktrees");
        let members = match self.recheck() {
            Ok(members) => members,
            Err(why) => return self.done("remove_worktrees", Err(why)),
        };
        let mut problems = Vec::new();
        let mut notes = Vec::new();
        let mut repos: Vec<&Path> = Vec::new();
        for member in live(&members).filter(|m| m.mode == Mode::Worktree) {
            if let Some(branch) = &member.branch {
                self.report.branches_kept.push(branch.clone());
                self.report
                    .unpushed
                    .extend(unpushed(self.ctx, member, branch));
            }
            match remove_worktree(self.ctx, member) {
                Ok(note) => notes.push(note),
                Err(why) => problems.push(why),
            }
            if let Some(repo) = &member.repo {
                repos.push(repo);
            }
        }
        for repo in repos {
            // Forget worktree records whose directory is gone; never a force.
            git::run(self.ctx.platform, repo, &["worktree", "prune"]).ok();
        }
        self.done_all(
            "remove_worktrees",
            &notes,
            &problems,
            "no worktrees to remove",
        )
    }

    fn close_places(&mut self) -> bool {
        (self.observer)("close_places");
        let members = match self.recheck() {
            Ok(members) => members,
            Err(why) => return self.done("close_places", Err(why)),
        };
        let mut problems = Vec::new();
        let mut notes = Vec::new();
        for member in live(&members) {
            match close_place(self.ctx, member) {
                Ok(Some(note)) => notes.push(note),
                Ok(None) => {}
                Err(why) => problems.push(why),
            }
        }
        self.done_all("close_places", &notes, &problems, "no places recorded")
    }

    fn release_hold(&mut self) -> bool {
        (self.observer)("release_hold");
        let (team, _) = match read(self.ctx) {
            Ok(pair) => pair,
            Err(why) => return self.done("release_hold", Err(why)),
        };
        let result = hold::release(self.ctx, &team);
        self.done("release_hold", result)
    }

    /// The team becomes `closed` here, and `team.closed` is written.
    fn finish(&mut self) -> bool {
        (self.observer)("finish");
        let now = self.ctx.clock.now();
        let result = match self
            .ctx
            .store
            .write(|tx| tx.finish_close(now, &CloseEnd::Completed))
        {
            Ok(true) => Ok("the team is closed".to_owned()),
            Ok(false) => Err("the close lock is no longer held".to_owned()),
            Err(e) => Err(format!("the team could not be marked closed: {e}")),
        };
        self.done("finish", result)
    }

    /// What follows a closed team: the archive gets a second backup so its
    /// database holds `team.closed`, then the state directory goes last.
    /// Every step is safe to run again on a closed team.
    fn finish_resumed(&mut self) {
        if self.report.archive.is_none() {
            // A rerun on a team that is already closed: nothing in the
            // archive yet, so the whole archive is taken and checked first.
            if !(self.archive() && self.verify_archive(true)) {
                return;
            }
        } else if !self.refresh_archive() {
            return;
        }
        self.delete_state();
    }

    fn refresh_archive(&mut self) -> bool {
        (self.observer)("archive_final");
        let Some(dest) = self.report.archive.clone() else {
            return self.done("archive_final", Err("there is no archive".to_owned()));
        };
        let result = archive::refresh_db(self.ctx, &dest)
            .and_then(|()| archive::verify(self.ctx, &self.team_dir, &dest, true))
            .map(|()| "the archived database now holds team.closed".to_owned());
        self.done("archive_final", result)
    }

    fn delete_state(&mut self) -> bool {
        (self.observer)("delete_state");
        if self.opts.keep_state {
            return self.done(
                "delete_state",
                Ok(format!("kept {}, marked closed", self.team_dir.display())),
            );
        }
        // The last look before the only copy goes: everything still on disk
        // must be in the archive, byte for byte.
        let Some(dest) = self.report.archive.clone() else {
            return self.done("delete_state", Err("there is no archive".to_owned()));
        };
        if let Err(why) = archive::verify(self.ctx, &self.team_dir, &dest, true) {
            return self.done("delete_state", Err(why));
        }
        let result = std::fs::remove_dir_all(&self.team_dir)
            .map(|()| format!("deleted {}", self.team_dir.display()))
            .map_err(|e| format!("cannot delete {}: {e}", self.team_dir.display()));
        self.done("delete_state", result)
    }

    /// One step over many members: every member is tried, and any problem
    /// stops the sequence after the loop.
    fn done_all(&mut self, step: &str, notes: &[String], problems: &[String], none: &str) -> bool {
        if problems.is_empty() {
            let detail = if notes.is_empty() {
                none.to_owned()
            } else {
                notes.join("; ")
            };
            self.done(step, Ok(detail))
        } else {
            self.done(step, Err(problems.join("; ")))
        }
    }
}

/// `git worktree remove` for one member, without force; the branch stays.
fn remove_worktree(ctx: &Ctx<'_>, member: &Member) -> Result<String, String> {
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

/// Closes the member's own place through the member's own host (CLS-7).
/// A place the host no longer knows is closed already.
fn close_place(ctx: &Ctx<'_>, member: &Member) -> Result<Option<String>, String> {
    let Some(place) = &member.place else {
        return Ok(None);
    };
    let role = &member.role;
    let (_, host) = ctx
        .adapters(&Party::Member(role.clone()))
        .map_err(|e| format!("{role}: {e}"))?;
    match host.close(place, HOST_TIMEOUT) {
        Ok(()) => Ok(Some(format!("{role}'s place {} closed", place.id))),
        Err(e) if e.code == HostErrorCode::NotFound => Ok(Some(format!(
            "{role}'s place {} was already gone",
            place.id
        ))),
        Err(e) => Err(format!("{role}: closing place {}: {e}", place.id)),
    }
}

/// What a kept branch has that no remote has (CLS-8): commits ahead of its
/// upstream, or, with no upstream, every commit since the member started.
pub(crate) fn unpushed(ctx: &Ctx<'_>, member: &Member, branch: &str) -> Vec<String> {
    if member.state == MemberState::Failed || !member.cwd.is_dir() {
        return Vec::new();
    }
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
                "{role}: branch {branch} is {ahead} commit(s) ahead of its upstream, not pushed"
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
            "{role}: branch {branch} has no upstream; {n} commit(s) exist only on this machine"
        )],
        _ => Vec::new(),
    }
}
