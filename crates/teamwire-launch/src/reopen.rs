//! Reopen: resume the sessions of a team whose sessions are gone. Every
//! session is checked first. The first tab then opens alone, the lead when it
//! is reopened, so a host that groups a team's tabs has a live tab to put the
//! others beside. The rest open at the same time: each waits seconds for its
//! session to load, and those waits do not depend on each other.
//!
//! Never: type into a reopened session; reopen a session that is running or
//! whose state is unknown (two processes on one session would make delivery
//! refuse with `two_places`); write anything on a dry run; write a token.
//!
//! Invariants: a member's identity lives in its per-run files, which a
//! resume reuses as they are; the session id never changes; only the
//! member's recorded place does.

use std::path::PathBuf;
use std::time::Duration;

use teamwire_core::{
    Address, Harness, Host, Liveness, Member, MemberLaunch, MemberState, Place, Role, SpawnRequest,
    StartupOutcome, TeamState, TrustPolicy,
};
use teamwire_store::{Ctx, RunOptions};
use thiserror::Error;

use crate::member_steps::spawn;
use crate::reopen_lead::reopen_lead;
use crate::{hold, watch_command};

const ALIVE_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

/// What to reopen.
#[derive(Debug, Clone, Default)]
pub struct ReopenOpts {
    /// Reopen only this member. The lead is then left alone.
    pub role: Option<Role>,
    /// Say what would happen; open nothing and write nothing.
    pub dry_run: bool,
    /// The user's home directory. The lead is never reopened there, because
    /// reopen would answer Claude's folder-trust dialog for it (LCH-15).
    pub user_home: Option<PathBuf>,
}

/// How one session's reopen ended. On a dry run `Reopened` means "would be".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReopenOutcome {
    Reopened,
    /// Nothing to do, or nothing safe to do.
    Skipped(String),
    /// Cannot be reopened as recorded; the reason says what is missing.
    Refused(String),
    /// The host or the harness failed.
    Failed(String),
}

impl ReopenOutcome {
    pub(crate) fn skipped(why: impl Into<String>) -> Self {
        Self::Skipped(why.into())
    }

    pub(crate) fn refused(why: impl Into<String>) -> Self {
        Self::Refused(why.into())
    }

    pub(crate) fn failed(why: impl Into<String>) -> Self {
        Self::Failed(why.into())
    }

    /// True for the two outcomes that make the command exit non-zero.
    pub fn is_problem(&self) -> bool {
        matches!(self, Self::Refused(_) | Self::Failed(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReopenReport {
    /// `None` when `--role` named a member, so the lead was not looked at.
    pub lead: Option<ReopenOutcome>,
    pub members: Vec<(Role, ReopenOutcome)>,
    pub warnings: Vec<String>,
    pub watch_command: String,
    pub dry_run: bool,
}

impl ReopenReport {
    pub fn has_problem(&self) -> bool {
        self.lead.iter().any(ReopenOutcome::is_problem)
            || self.members.iter().any(|(_, o)| o.is_problem())
    }
}

/// Why nothing was attempted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReopenError {
    #[error("the team has no record")]
    NoTeam,
    #[error("team {team} is {state}; only an open team can be reopened")]
    NotOpen { team: String, state: TeamState },
    #[error("no member {0} in this team")]
    NoMember(Role),
    #[error("the store could not be read: {0}")]
    Store(String),
}

/// Reopens the lead and the members of the team in `ctx`, as `opts` says.
/// One session's failure never stops the others.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub fn reopen(ctx: &mut Ctx<'_>, opts: &ReopenOpts) -> Result<ReopenReport, ReopenError> {
    let store_error = |e: teamwire_store::StoreError| ReopenError::Store(e.to_string());
    let team = ctx
        .store
        .read(|tx| tx.team())
        .map_err(store_error)?
        .ok_or(ReopenError::NoTeam)?;
    if team.state != TeamState::Open {
        return Err(ReopenError::NotOpen {
            team: team.name.to_string(),
            state: team.state,
        });
    }
    let members = ctx
        .store
        .read(|tx| tx.members()?.all_or_corrupt())
        .map_err(store_error)?;
    if let Some(wanted) = &opts.role
        && !members.iter().any(|m| &m.role == wanted)
    {
        return Err(ReopenError::NoMember(wanted.clone()));
    }
    let mut report = ReopenReport {
        lead: None,
        members: Vec::new(),
        warnings: Vec::new(),
        watch_command: watch_command(&team.name),
        dry_run: opts.dry_run,
    };
    // `None` is the lead. Each entry passed every check and waits for its tab.
    let mut ready: Vec<(Option<Role>, Ready<'_>)> = Vec::new();
    if opts.role.is_none() {
        let mut lead = None;
        report.lead = Some(reopen_lead(ctx, &team, opts, &mut lead));
        ready.extend(lead.map(|r| (None, r)));
    }
    for member in members
        .iter()
        .filter(|m| opts.role.as_ref().is_none_or(|r| *r == m.role))
    {
        let options = ctx
            .store
            .read(|tx| tx.run_options(&member.role))
            .map_err(store_error)?;
        let mut one = None;
        let outcome = reopen_member(
            ctx,
            &team,
            member,
            &options,
            opts,
            &mut report.warnings,
            &mut one,
        );
        ready.extend(one.map(|r| (Some(member.role.clone()), r)));
        report.members.push((member.role.clone(), outcome));
    }
    let opened = open_all(&ready);
    for ((who, r), result) in ready.iter().zip(opened) {
        let outcome = match (result, who) {
            (Err(outcome), _) => outcome,
            (Ok(_), None) => ReopenOutcome::Reopened,
            (Ok(place), Some(role)) => record_place(ctx, role, r, &place),
        };
        match who {
            None => report.lead = Some(outcome),
            Some(role) => {
                if let Some(entry) = report.members.iter_mut().find(|(r, _)| r == role) {
                    entry.1 = outcome;
                }
            }
        }
    }
    let opened = report.lead == Some(ReopenOutcome::Reopened)
        || report
            .members
            .iter()
            .any(|(_, o)| *o == ReopenOutcome::Reopened);
    if opened && !opts.dry_run {
        let lead = (&team.lead_harness, team.lead_session.as_ref());
        hold::ensure_hold(ctx, &team.name, lead, &mut report.warnings);
    }
    Ok(report)
}

/// Checks one member. A member that passes is `Reopened` (on a dry run, "would
/// be") and, unless this is a dry run, leaves its tab request in `ready`.
fn reopen_member<'r>(
    ctx: &Ctx<'r>,
    team: &teamwire_store::TeamRow,
    member: &Member,
    options: &RunOptions,
    opts: &ReopenOpts,
    warnings: &mut Vec<String>,
    ready: &mut Option<Ready<'r>>,
) -> ReopenOutcome {
    match member.state {
        MemberState::Started => {}
        MemberState::Removed => return ReopenOutcome::skipped("removed"),
        MemberState::Failed => return ReopenOutcome::skipped("it failed to start"),
        MemberState::Creating => return ReopenOutcome::skipped("it never finished starting"),
    }
    let Some(session) = &member.session_id else {
        return ReopenOutcome::refused("no session is recorded, so there is nothing to resume");
    };
    let registry = ctx.registry;
    let (harness, host) = match (
        registry.harness(&member.harness),
        registry.host(&member.host),
    ) {
        (Ok(harness), Ok(host)) => (harness, host),
        (Err(e), _) => return ReopenOutcome::refused(e.to_string()),
        (_, Err(e)) => return ReopenOutcome::refused(e.to_string()),
    };
    match harness.is_alive(session, ALIVE_TIMEOUT) {
        Ok(Liveness::Dead) => {}
        Ok(Liveness::Alive(_)) => return ReopenOutcome::skipped("already open"),
        Ok(Liveness::Unknown) => return ReopenOutcome::skipped(UNKNOWN),
        Err(e) => {
            return ReopenOutcome::skipped(format!(
                "{UNKNOWN} ({e}); two processes on one session would make delivery refuse"
            ));
        }
    }
    if !member.cwd.is_dir() {
        return ReopenOutcome::refused(format!(
            "its working directory {} is gone",
            member.cwd.display()
        ));
    }
    let team_dir = ctx.home.join("teams").join(team.name.as_str());
    let launch = MemberLaunch::for_resume(
        &team_dir,
        &team.name,
        member,
        session,
        (options.model.clone(), options.effort.clone()),
    );
    let spec = match harness.resume_spec(&launch, session) {
        Ok(spec) => spec,
        Err(e) => {
            return ReopenOutcome::failed(format!("the harness could not build the resume: {e}"));
        }
    };
    if spec.identity_channel == teamwire_core::IdentityChannel::HostEnv {
        return ReopenOutcome::refused(
            "its identity travels in the host's environment and its token is not stored",
        );
    }
    if let Some(file) = spec.files.iter().find(|f| !f.path.is_file()) {
        return ReopenOutcome::refused(format!("its file {} is missing", file.path.display()));
    }
    if !options.recorded {
        warnings.push(format!(
            "{} has no model or effort recorded (an older build started it), so it resumes on Claude's default",
            member.role
        ));
    }
    if opts.dry_run {
        return ReopenOutcome::Reopened;
    }
    let request = SpawnRequest {
        name: Address::of(&team.name, &member.role),
        title: member.title.clone(),
        cwd: member.cwd.clone(),
        argv: spec.argv,
        env: Vec::new(),
    };
    *ready = Some(Ready {
        harness,
        host,
        request,
        folder: member.repo.clone().unwrap_or_else(|| member.cwd.clone()),
    });
    ReopenOutcome::Reopened
}

const UNKNOWN: &str = "it is not known whether the session runs";

/// A session that passed every check, with what its new tab needs.
pub(crate) struct Ready<'r> {
    pub(crate) harness: &'r dyn Harness,
    pub(crate) host: &'r dyn Host,
    pub(crate) request: SpawnRequest,
    /// Named in the advice when the folder's settings escalate the trust dialog.
    pub(crate) folder: PathBuf,
}

/// Opens one tab per entry, in order, and returns one result per entry. Tabs
/// open one at a time until one is up; the rest then open at the same time.
/// herdr puts a new tab beside a live agent of the team, so tabs opened
/// together before any is live would each start a workspace of their own.
fn open_all(ready: &[(Option<Role>, Ready<'_>)]) -> Vec<Result<Place, ReopenOutcome>> {
    let mut opened = Vec::with_capacity(ready.len());
    let mut rest = ready.iter();
    for (_, r) in rest.by_ref() {
        let result = open_place(r);
        let up = result.is_ok();
        opened.push(result);
        if up {
            break;
        }
    }
    let rest: Vec<_> = rest.collect();
    std::thread::scope(|s| {
        let threads: Vec<_> = rest
            .iter()
            .map(|entry| {
                let r = &entry.1;
                s.spawn(move || open_place(r))
            })
            .collect();
        opened.extend(threads.into_iter().map(|t| {
            t.join().unwrap_or_else(|_| {
                Err(ReopenOutcome::failed(
                    "the step that opened this tab stopped; look for a new tab in the host and \
                     close it before running reopen again",
                ))
            })
        }));
    });
    opened
}

/// Starts the request in a new tab and answers a plain folder-trust dialog as
/// launch does by default. On a failure after the spawn the new tab is
/// closed, as launch's undo does, so a second reopen cannot start the same
/// session twice. The stronger trust dialog is never answered.
pub(crate) fn open_place(r: &Ready<'_>) -> Result<Place, ReopenOutcome> {
    let place = match spawn(r.host, &r.request) {
        Ok(place) => place,
        Err(stop) if stop.refused => return Err(ReopenOutcome::Refused(stop.reason)),
        Err(stop) => return Err(ReopenOutcome::Failed(stop.reason)),
    };
    let failed = match r
        .harness
        .answer_startup(r.host, &place, TrustPolicy::Accept)
    {
        Ok(StartupOutcome::NoDialog | StartupOutcome::AnsweredPlainTrust) => return Ok(place),
        Ok(StartupOutcome::RefusedEscalated) => format!(
            "the folder's own settings ask for extra permissions, so the trust dialog was not \
             answered; open Claude Code yourself in {}, review and accept that dialog there, \
             then run reopen again",
            r.folder.display()
        ),
        Err(e) => format!("the trust step failed: {e}"),
    };
    Err(close_new_tab(r.host, &place, &failed))
}

/// Records a member's new place. It runs after its tab is up, so a failed
/// reopen leaves the old record as it was.
fn record_place(ctx: &mut Ctx<'_>, role: &Role, r: &Ready<'_>, place: &Place) -> ReopenOutcome {
    match ctx.store.write(|tx| tx.set_member_place(role, place)) {
        Ok(()) => ReopenOutcome::Reopened,
        Err(e) => close_new_tab(
            r.host,
            place,
            &format!("the place could not be recorded: {e}"),
        ),
    }
}

fn close_new_tab(host: &dyn Host, place: &Place, failed: &str) -> ReopenOutcome {
    match host.close(place, CLOSE_TIMEOUT) {
        Ok(()) => ReopenOutcome::Failed(format!("{failed}; the new tab was closed")),
        Err(e) => ReopenOutcome::Failed(format!(
            "{failed}; the new tab {} could not be closed ({e}): close it by hand before running \
             reopen again",
            place.id
        )),
    }
}
