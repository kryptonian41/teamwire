//! A lead handoff before anything changes (ADR 0051): who is calling and
//! which open teams it leads, then every refusal, then the new lead's
//! command line and first prompt. Every store is opened read-only.
//!
//! Never: write a store, open a pane or start a process here. A refusal
//! leaves every team with the old lead.

use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::{
    Clock, Config, HandoffFiles, Harness, HarnessId, Host, HostId, LaunchSpec, LeadStart,
    LeadStartSpec, NoHost, Place, Platform, Registry, SessionId, SessionKind, SplitRequest,
    TeamName,
};
use teamwire_deliver::locate_session;
use teamwire_store::{Store, TeamRow, open_teams};

use crate::handoff::HandoffError;
use crate::lead_handoff::LeadHandoffOpts;
use crate::lead_prompt::{FirstPrompt, first_prompt};

const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// What a lead handoff works with: the state directory and the adapters.
/// It names no team, because the command finds the caller's teams itself.
#[derive(Clone, Copy)]
pub struct LeadHandoffCtx<'a> {
    pub home: &'a Path,
    pub registry: &'a Registry,
    pub platform: &'a dyn Platform,
    pub clock: &'a dyn Clock,
    pub config: &'a Config,
}

impl std::fmt::Debug for LeadHandoffCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeadHandoffCtx")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

/// Who runs the command.
#[derive(Clone, Copy)]
pub struct LeadCaller<'a> {
    /// One variable of the caller's environment.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// The caller runs in a member's session: its role, team or token
    /// variable is set.
    pub is_member: bool,
}

impl std::fmt::Debug for LeadCaller<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeadCaller")
            .field("is_member", &self.is_member)
            .finish_non_exhaustive()
    }
}

/// The session that runs the command, and the open teams it leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadSeat {
    pub session: SessionId,
    pub harness: HarnessId,
    pub host: HostId,
    /// The led teams, sorted by name. Never empty.
    pub teams: Vec<TeamRow>,
}

impl LeadSeat {
    /// The led teams' names, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<TeamName> {
        self.teams.iter().map(|t| t.name.clone()).collect()
    }
}

/// Finds the calling session through each open team's lead harness, and
/// the open teams it leads. Refused for a member's session, a background
/// session, a caller that leads no open team, and teams of one lead that
/// record different lead harnesses or hosts. A team whose store cannot be
/// read is `Store`: it may be one of the caller's.
pub fn find_lead(
    ctx: &LeadHandoffCtx<'_>,
    caller: &LeadCaller<'_>,
) -> Result<LeadSeat, HandoffError> {
    if caller.is_member {
        return Err(HandoffError::Refused(
            "this runs in a member's session; only a team's lead can hand off its teams".to_owned(),
        ));
    }
    let teams = open_teams(ctx.home, None).map_err(|e| HandoffError::Store(e.to_string()))?;
    let mut led: Vec<(SessionKind, TeamRow)> = Vec::new();
    for (name, read) in teams {
        let team = match read {
            Ok(Some(team)) => team.row,
            Ok(None) => continue,
            Err(e) => return Err(HandoffError::Store(format!("team {name}: {e}"))),
        };
        // A team whose lead harness this binary lacks cannot name the caller.
        let Ok(harness) = ctx.registry.harness(&team.lead_harness) else {
            continue;
        };
        if let Some(me) = harness.caller_session(caller.env)
            && team.lead_session.as_ref() == Some(&me.id)
        {
            led.push((me.kind, team));
        }
    }
    let Some((_, first)) = led.first() else {
        return Err(HandoffError::Refused(
            "this session leads no open team, so there is nothing to hand off".to_owned(),
        ));
    };
    let (session, harness, host) = (
        first.lead_session.clone(),
        first.lead_harness.clone(),
        first.lead_host.clone(),
    );
    if led.iter().any(|(kind, team)| {
        *kind == SessionKind::Background || team.lead_kind == Some(SessionKind::Background)
    }) {
        return Err(HandoffError::Refused(
            "this is a background session; only an interactive lead can hand off".to_owned(),
        ));
    }
    if led
        .iter()
        .any(|(_, t)| t.lead_harness != harness || t.lead_host != host)
    {
        return Err(HandoffError::Refused(
            "the teams this session leads record different lead harnesses or hosts; hand them \
             off one at a time with `teamwire lead --adopt <team>` from a new lead"
                .to_owned(),
        ));
    }
    let Some(session) = session else {
        return Err(HandoffError::Refused(
            "the led team records no lead session".to_owned(),
        ));
    };
    Ok(LeadSeat {
        session,
        harness,
        host,
        teams: led.into_iter().map(|(_, team)| team).collect(),
    })
}

/// A handoff that passed every check: what it will run, and where.
pub(crate) struct Planned<'r> {
    pub(crate) seat: LeadSeat,
    pub(crate) harness: &'r dyn Harness,
    pub(crate) host: &'r dyn Host,
    pub(crate) files: HandoffFiles,
    pub(crate) old_place: Place,
    pub(crate) new_session: SessionId,
    pub(crate) split: SplitRequest,
    pub(crate) dropped: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

/// Runs every check of step 2 and builds the new lead's start. The new
/// pane gets the state directory the caller named and the config folder of
/// the lead's own harness, from `opts`.
pub(crate) fn plan<'r>(
    ctx: &LeadHandoffCtx<'r>,
    seat: LeadSeat,
    opts: &LeadHandoffOpts,
) -> Result<Planned<'r>, HandoffError> {
    let refused = |why: String| HandoffError::Refused(why);
    let harness = ctx
        .registry
        .harness(&seat.harness)
        .map_err(|e| refused(e.to_string()))?;
    let host_id = seat.host.clone();
    let old = seat.session.clone();
    let files = HandoffFiles::of(ctx.home, &old).ok_or_else(|| {
        refused(format!(
            "the session id {old} is not a safe folder name, so it has no handoff folder"
        ))
    })?;
    let cwd = lead_folder(ctx.home, harness, &seat)?;
    let caps = harness.capabilities();
    let new_session =
        SessionId::parse(&uuid::Uuid::new_v4().to_string()).map_err(|e| refused(e.to_string()))?;
    let mut warnings = Vec::new();
    let old_args = old_args(ctx.platform, harness, &old).unwrap_or_else(|| {
        // A pi lead's approval flags live only in its arguments, so
        // the user must hear that none of them reach the new lead.
        warnings.push(
            "the old lead's arguments could not be read, so the new lead starts with no flag \
             carried from them"
                .to_owned(),
        );
        Vec::new()
    });
    let start = |adopt_first: bool| {
        let prompt = first_prompt(
            &caps,
            &FirstPrompt {
                old: &old,
                teams: &seat.teams,
                document: &files.document,
                adopt_first,
            },
        );
        harness.lead_start(&LeadStart {
            old_session: old.clone(),
            old_args: old_args.clone(),
            new_session: new_session.clone(),
            prompt,
        })
    };
    let spec: LeadStartSpec = start(false).map_err(|e| {
        refused(format!(
            "the {} harness cannot start a new lead: {e}",
            seat.harness
        ))
    })?;
    let host = ctx
        .registry
        .host(&host_id)
        .map_err(|e| refused(e.to_string()))?;
    if host_id.as_str() == NoHost::ID || !host.capabilities().splits_pane {
        // Nothing records a lead the user starts by hand, so its prompt
        // tells it to adopt each team first.
        let by_hand = start(true).map_or_else(|_| spec.argv.clone(), |s| s.argv);
        return Err(refused(format!(
            "the lead's host {host_id} cannot open a pane beside this one; to start the new lead \
             by hand, run: cd {} && {}",
            LaunchSpec::shell_line(&[cwd.display().to_string()]),
            LaunchSpec::shell_line(&by_hand)
        )));
    }
    // The harness's own warnings are said with the plan's, on a dry run too.
    warnings.extend(spec.warnings);
    document_checks(&files, &mut warnings)?;
    let old_place = locate_session(ctx.platform, ctx.clock, &old, harness, host).map_err(|e| {
        refused(format!(
            "this lead's pane cannot be located ({}): {}",
            e.code, e.detail
        ))
    })?;
    let mut env = Vec::new();
    if let Some(home) = &opts.home_env {
        env.push(("TEAMWIRE_HOME".to_owned(), home.display().to_string()));
    }
    for (harness, name, value) in &opts.harness_env {
        if *harness == seat.harness {
            env.push((name.clone(), value.display().to_string()));
        }
    }
    let title = seat.teams.first().map_or("", |t| t.title.as_str());
    let split = SplitRequest {
        label: SplitRequest::lead_label(title, seat.teams.len().saturating_sub(1)),
        cwd,
        argv: spec.argv,
        env,
    };
    Ok(Planned {
        seat,
        harness,
        host,
        files,
        old_place,
        new_session,
        split,
        dropped: spec.dropped,
        warnings,
    })
}

/// The old lead's folder: what its harness reads of the session, else the
/// folder recorded for the first led team's lead. A new session of a
/// harness that finds sessions by folder (pi) must start there.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::lead_cwd` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn lead_folder(
    home: &Path,
    harness: &dyn Harness,
    seat: &LeadSeat,
) -> Result<PathBuf, HandoffError> {
    let recorded = || {
        let team = seat.teams.first()?;
        let store = Store::open_read_only(home, &team.name).ok()?;
        store.read(|tx| tx.lead_cwd()).ok().flatten()
    };
    let cwd = harness
        .session_cwd(&seat.session)
        .ok()
        .flatten()
        .or_else(recorded)
        .ok_or_else(|| {
            HandoffError::Refused(
                "this lead's folder is not known: the harness does not name it and no team \
                 recorded it"
                    .to_owned(),
            )
        })?;
    if !cwd.is_dir() {
        return Err(HandoffError::Refused(format!(
            "this lead's folder {} is gone",
            cwd.display()
        )));
    }
    Ok(cwd)
}

/// The old lead process's arguments, or `None` when its process or its
/// arguments cannot be read.
fn old_args(
    platform: &dyn Platform,
    harness: &dyn Harness,
    old: &SessionId,
) -> Option<Vec<String>> {
    let pids = harness
        .processes_for_session(old, LIST_TIMEOUT)
        .unwrap_or_default();
    pids.iter().min().and_then(|pid| platform.args_of(*pid))
}

/// The handoff document must exist. One older than the note may describe
/// an earlier moment, so it is named in a warning.
fn document_checks(files: &HandoffFiles, warnings: &mut Vec<String>) -> Result<(), HandoffError> {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified());
    let written = modified(&files.document).map_err(|e| {
        HandoffError::Refused(format!(
            "the handoff document {} cannot be read ({e}); run `teamwire lead --handoff-guide` \
             and write it first",
            files.document.display()
        ))
    })?;
    if let Ok(noted) = modified(&files.noted)
        && written < noted
    {
        warnings.push(format!(
            "the handoff document {} is older than the note that your context passed its \
             handoff point; check that it describes the present state",
            files.document.display()
        ));
    }
    Ok(())
}
