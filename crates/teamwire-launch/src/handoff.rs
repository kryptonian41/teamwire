//! Handoff, the launch half: a fresh session for a started member in the
//! same seat. `hand_off` runs `open_handoff` after close has taken the old
//! place down, because the host refuses a second live agent of one name. The row,
//! worktree, branch, token, per-run files, brief and report stay; the
//! session, the place and the generation change, in one store write made
//! after the trust step.
//!
//! Never: write the per-run files (the spec's token is a placeholder and the
//! real files exist); type into the old session; send a reprompt (the store
//! write already started the new generation).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use teamwire_core::{
    Address, DeliveryState, Member, MemberState, Mode, Place, Recipient, Role, SessionId,
    SpawnRequest, TeamName, TeamState, Timestamp,
};
use teamwire_store::Ctx;
use thiserror::Error;

use crate::member_steps::{first_line, not_open};
use crate::reopen::{
    Ready, ReopenOpts, ReopenOutcome, close_new_tab, member_launch, open_place, reusable,
};

/// A member that passed launch's checks, with the tab its new session needs.
pub struct HandoffPlan<'r> {
    pub team: TeamName,
    pub member: Member,
    /// The id the new session starts with, for a harness that lets teamwire
    /// choose it. `None` for one that picks its own, whose session-start
    /// hook records it.
    pub new_session: Option<SessionId>,
    /// The argv of the new session, for `--dry-run`.
    pub argv: Vec<String>,
    /// The new session's first line.
    pub first_line: String,
    /// For the lead: the handoff document is missing or out of date, so the
    /// first line points at the report and the git log instead.
    pub warnings: Vec<String>,
    ready: Ready<'r>,
}

/// Why a handoff stopped. `plan_handoff` returns only the first two, which
/// touch nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HandoffError {
    #[error("{0}")]
    Refused(String),
    #[error("the store could not be read: {0}")]
    Store(String),
    /// The old place's close began, and no new session took over. The text
    /// says how to get the member back.
    #[error("{0}")]
    Partial(String),
}

impl std::fmt::Debug for HandoffPlan<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandoffPlan")
            .field("team", &self.team)
            .field("member", &self.member.role)
            .field("new_session", &self.new_session)
            .field("argv", &self.argv)
            .finish_non_exhaustive()
    }
}

/// What `open_handoff` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    pub place: Place,
    pub generation: u32,
    pub warnings: Vec<String>,
}

/// Checks that `role` can get a fresh session, and builds its tab request:
/// the team is open, the member started, its folder and per-run files are
/// there. Writes nothing.
pub fn plan_handoff<'r>(
    ctx: &Ctx<'r>,
    role: &Role,
    teamwire_bin: Option<PathBuf>,
) -> Result<HandoffPlan<'r>, HandoffError> {
    let (team, member, options, unread) = ctx
        .store
        .read(|tx| {
            let unread = tx.deliveries_for(&Recipient::Member(role.clone()), &UNREAD)?;
            Ok((
                tx.team()?,
                tx.member(role)?,
                tx.run_options(role)?,
                unread.items.len(),
            ))
        })
        .map_err(|e| HandoffError::Store(e.to_string()))?;
    plan_with(ctx, role, teamwire_bin, (team, member, options, unread))
        .map_err(HandoffError::Refused)
}

/// Every delivery state but `acked`: what `teamwire inbox` still returns,
/// including a short message typed whole into the old pane, which the old
/// session may never have read.
const UNREAD: [DeliveryState; 4] = [
    DeliveryState::Pending,
    DeliveryState::HandedOff,
    DeliveryState::Unconfirmed,
    DeliveryState::Undelivered,
];

type Read = (
    Option<teamwire_store::TeamRow>,
    Option<Member>,
    teamwire_store::RunOptions,
    usize,
);

fn plan_with<'r>(
    ctx: &Ctx<'r>,
    role: &Role,
    teamwire_bin: Option<PathBuf>,
    (team, member, options, unread): Read,
) -> Result<HandoffPlan<'r>, String> {
    let team = team.ok_or("the team has no record")?;
    if team.state != TeamState::Open {
        return Err(format!(
            "team {} is {}; only an open team can hand off",
            team.name, team.state
        ));
    }
    let member = member.ok_or_else(|| format!("no member {role} in team {}", team.name))?;
    if member.state != MemberState::Started {
        return Err(format!(
            "{role} is {}; only a started member can hand off",
            member.state
        ));
    }
    let registry = ctx.registry;
    let harness = registry
        .harness(&member.harness)
        .map_err(|e| e.to_string())?;
    let host = registry.host(&member.host).map_err(|e| e.to_string())?;
    if !member.cwd.is_dir() {
        return Err(format!(
            "{role}'s working directory {} is gone",
            member.cwd.display()
        ));
    }
    let session = SessionId::parse(&uuid::Uuid::new_v4().to_string()).map_err(|e| e.to_string())?;
    let opts = ReopenOpts {
        teamwire_bin,
        ..ReopenOpts::default()
    };
    let launch = member_launch(ctx, &team.name, &member, &options, &opts, &session)
        .map_err(|o| format!("{role}: {}", reason(o)))?;
    let spec = harness
        .launch_spec(&launch)
        .map_err(|e| format!("{role}: the harness could not build the launch: {e}"))?;
    reusable(&spec).map_err(|o| format!("{role}: {}", reason(o)))?;
    let team_dir = ctx.home.join("teams").join(team.name.as_str());
    let brief = if member.mode == Mode::Worktree {
        member.cwd.join("TASK.md")
    } else {
        team_dir.join("briefs").join(format!("{role}.md"))
    };
    let reports = team_dir.join("reports");
    let handoff = reports.join(format!("{role}.handoff.md"));
    let mut warnings = Vec::new();
    let mut first_line = match stale_document(&handoff, member.gen_started_at) {
        None => format!(
            "This is a fresh session in your seat. Your brief is at {} and your previous \
             session's handoff document is at {}. Read both in full now, then carry on from \
             where the handoff document says.",
            brief.display(),
            handoff.display()
        ),
        Some(why) => {
            warnings.push(format!(
                "{role}: {why}, so the new session is pointed at its report and git log instead"
            ));
            let log = member
                .branch
                .as_ref()
                .map_or_else(String::new, |b| format!(" and `git log {b}`"));
            format!(
                "This is a fresh session in your seat. Your brief is at {}. Your previous \
                 session left no current handoff document, so read the brief, your report {}{log} \
                 to see what is done, then carry on.",
                brief.display(),
                reports.join(format!("{role}.md")).display()
            )
        }
    };
    if unread > 0 {
        write!(first_line, " {}", unread_note(unread)).ok();
    }
    let request = SpawnRequest {
        name: Address::of(&team.name, role),
        title: member.title.clone(),
        team_title: team.title.clone(),
        // `record_handoff` starts this generation once the new tab is open.
        generation: member.generation + 1,
        cwd: member.cwd.clone(),
        argv: spec.argv.clone(),
        env: Vec::new(),
    };
    let ready = Ready {
        harness,
        host,
        request,
        folder: member.repo.clone().unwrap_or_else(|| member.cwd.clone()),
    };
    Ok(HandoffPlan {
        new_session: harness.capabilities().choose_session_id.then_some(session),
        team: team.name,
        member,
        argv: spec.argv,
        first_line,
        warnings,
        ready,
    })
}

/// Opens the new tab, answers the trust dialog, records the handoff and
/// sends the first line. Run only once the old place is closed. On an error
/// the old session is still recorded, and the error says how to resume it.
pub fn open_handoff(ctx: &mut Ctx<'_>, plan: &HandoffPlan<'_>) -> Result<Handoff, String> {
    let role = &plan.member.role;
    let place = match open_place(&plan.ready) {
        Ok(place) => place,
        Err(o) => return Err(failed(ctx, plan, &reason(o))),
    };
    let at = ctx.clock.now();
    let old = plan.member.session_id.as_ref();
    let written = ctx.store.write(|tx| {
        if let Some(state) = not_open(&tx.reader())? {
            return Ok(Err(state));
        }
        // A harness that picks its own session may have recorded it already
        // through its session-start hook; that id is the new session's.
        let session = match &plan.new_session {
            Some(s) => Some(s.clone()),
            None => tx
                .reader()
                .member(role)?
                .and_then(|m| m.session_id)
                .filter(|s| Some(s) != old),
        };
        tx.record_handoff(at, role, session.as_ref(), &place)
            .map(Ok)
    });
    let generation = match written {
        Ok(Ok(generation)) => generation,
        Ok(Err(state)) => {
            let why = format!("team_closed: the team is {state}");
            let why = close_new_tab(&plan.ready, &place, &why);
            return Err(failed(ctx, plan, &why));
        }
        Err(e) => {
            let why = format!("the handoff could not be recorded: {e}");
            let why = close_new_tab(&plan.ready, &place, &why);
            return Err(failed(ctx, plan, &why));
        }
    };
    let mut warnings = Vec::new();
    first_line(
        plan.ready.host,
        &place,
        &plan.first_line,
        (&plan.team, role),
        &mut warnings,
    );
    Ok(Handoff {
        place,
        generation,
        warnings,
    })
}

/// The sentence that tells a fresh session about its unread messages. The
/// member tool comes first: a member's shell may have no `teamwire` on its
/// PATH, so the command it can run is the one its contract names.
fn unread_note(unread: usize) -> String {
    format!(
        "You have {unread} unread message(s): read them with the team_inbox tool, or with the \
         `teamwire inbox` command your contract names."
    )
}

/// Why the handoff document at `path` cannot be trusted, if it cannot: it is
/// missing, or older than the generation it should describe, such as one a
/// member that handed off before left behind.
fn stale_document(path: &Path, generation_started: Timestamp) -> Option<String> {
    let written = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(written) => written,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Some(format!(
                "the handoff document {} is missing",
                path.display()
            ));
        }
        Err(e) => {
            return Some(format!(
                "the handoff document {} cannot be read ({e})",
                path.display()
            ));
        }
    };
    let written = Timestamp::from_datetime(time::OffsetDateTime::from(written));
    (written < generation_started).then(|| {
        format!(
            "the handoff document {} was written before this session started ({written}, \
             session started {generation_started})",
            path.display()
        )
    })
}

fn reason(outcome: ReopenOutcome) -> String {
    match outcome {
        ReopenOutcome::Reopened => "reopened".to_owned(),
        ReopenOutcome::Skipped(why) | ReopenOutcome::Refused(why) | ReopenOutcome::Failed(why) => {
            why
        }
    }
}

/// `why`, and how to get the member back. Nothing recorded the handoff, but
/// a harness that records its own session (through its session-start hook)
/// may have put the new session on the row before the failure. The
/// old one is written back, after the new tab is closed, so reopen resumes
/// the session that holds the member's work.
fn failed(ctx: &mut Ctx<'_>, plan: &HandoffPlan<'_>, why: &str) -> String {
    let role = &plan.member.role;
    let Some(old) = &plan.member.session_id else {
        return format!(
            "{role}: {why}. No session was recorded for {role} before the handoff, so there is \
             no old session for `teamwire reopen` to resume"
        );
    };
    let restored = ctx.store.write(|tx| {
        let now = tx.reader().member(role)?.and_then(|m| m.session_id);
        if now.as_ref() == Some(old) {
            return Ok(());
        }
        tx.set_member_session(role, old)
    });
    match restored {
        Ok(()) => format!(
            "{role}: {why}. The old session {old} is still recorded, so `teamwire reopen {} \
             --role {role}` resumes it",
            plan.team
        ),
        Err(e) => format!(
            "{role}: {why}. The old session {old} could not be recorded again ({e}), so the \
             member may hold a new, empty session; reopen would resume that one"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::unread_note;

    #[test]
    fn the_unread_note_names_the_tool_first_and_the_contracts_command() {
        assert_eq!(
            unread_note(2),
            "You have 2 unread message(s): read them with the team_inbox tool, or with the \
             `teamwire inbox` command your contract names."
        );
    }
}
