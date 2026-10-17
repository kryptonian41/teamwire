//! Reopen: resume the sessions of a team whose sessions are gone. The lead
//! first, so a host that groups a team's tabs puts the members beside it,
//! then each member that started and is no longer running.
//!
//! Never: type into a reopened session; reopen a session that is running or
//! whose state is unknown (two processes on one session would make delivery
//! refuse with `two_places`); write anything on a dry run; write a token.
//!
//! Invariants: a member's identity lives in its per-run files, which a
//! resume reuses as they are; the session id never changes; only the
//! member's recorded place does.

use std::time::Duration;

use teamwire_core::{
    Address, Harness, Host, Liveness, Member, MemberLaunch, MemberState, Role, SpawnRequest,
    StartupOutcome, TeamState, TrustPolicy,
};
use teamwire_store::{Ctx, RunOptions};
use thiserror::Error;

use crate::member_steps::spawn;
use crate::reopen_lead::reopen_lead;
use crate::{hold, watch_command};

const ALIVE_TIMEOUT: Duration = Duration::from_secs(5);

/// What to reopen.
#[derive(Debug, Clone, Default)]
pub struct ReopenOpts {
    /// Reopen only this member. The lead is then left alone.
    pub role: Option<Role>,
    /// Say what would happen; open nothing and write nothing.
    pub dry_run: bool,
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
    if opts.role.is_none() {
        report.lead = Some(reopen_lead(ctx, &team, opts));
    }
    for member in members
        .iter()
        .filter(|m| opts.role.as_ref().is_none_or(|r| *r == m.role))
    {
        let options = ctx
            .store
            .read(|tx| tx.run_options(&member.role))
            .map_err(store_error)?;
        let outcome = reopen_member(ctx, &team, member, &options, opts, &mut report.warnings);
        report.members.push((member.role.clone(), outcome));
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

fn reopen_member(
    ctx: &mut Ctx<'_>,
    team: &teamwire_store::TeamRow,
    member: &Member,
    options: &RunOptions,
    opts: &ReopenOpts,
    warnings: &mut Vec<String>,
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
    if options.model.is_none() && options.effort.is_none() {
        warnings.push(format!(
            "{} has no model or effort recorded, so it resumes on Claude's default",
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
    let place = match spawn(host, &request) {
        Ok(place) => place,
        Err(stop) if stop.refused => return ReopenOutcome::Refused(stop.reason),
        Err(stop) => return ReopenOutcome::Failed(stop.reason),
    };
    if let Err(e) = ctx
        .store
        .write(|tx| tx.set_member_place(&member.role, &place))
    {
        return ReopenOutcome::failed(format!(
            "it was reopened in {} but the place could not be recorded: {e}",
            place.id
        ));
    }
    match answer_trust(
        harness,
        host,
        &place,
        member.repo.as_ref().unwrap_or(&member.cwd),
    ) {
        Ok(()) => ReopenOutcome::Reopened,
        Err(why) => ReopenOutcome::Failed(why),
    }
}

const UNKNOWN: &str = "it is not known whether the session runs";

/// Answers a plain folder-trust dialog, as launch does by default. The
/// stronger dialog is never answered: the tab stays open at it.
pub(crate) fn answer_trust(
    harness: &dyn Harness,
    host: &dyn Host,
    place: &teamwire_core::Place,
    folder: &std::path::Path,
) -> Result<(), String> {
    match harness.answer_startup(host, place, TrustPolicy::Accept) {
        Ok(StartupOutcome::NoDialog | StartupOutcome::AnsweredPlainTrust) => Ok(()),
        Ok(StartupOutcome::RefusedEscalated) => Err(format!(
            "the folder's own settings ask for extra permissions, so the trust dialog was not \
             answered; the tab {} is still open at it: open Claude Code yourself in {}, review \
             and accept that dialog there, then run reopen again",
            place.id,
            folder.display()
        )),
        Err(e) => Err(format!("the trust step failed: {e}")),
    }
}
