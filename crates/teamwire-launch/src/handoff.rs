//! Handoff, the launch half: a fresh session for a started member in the
//! same seat. The CLI runs `open_handoff` after close has taken the old place
//! down, because the host refuses a second live agent of one name. The row,
//! worktree, branch, token, per-run files, brief and report stay; the
//! session, the place and the generation change, in one store write made
//! after the trust step.
//!
//! Never: write the per-run files (the spec's token is a placeholder and the
//! real files exist); type into the old session; send a reprompt (the store
//! write already started the new generation).

use std::path::PathBuf;

use teamwire_core::{
    Address, Member, MemberState, Mode, Place, Role, SessionId, SpawnRequest, TeamName, TeamState,
};
use teamwire_store::Ctx;

use crate::member_steps::{first_line, not_open};
use crate::reopen::{
    Ready, ReopenOpts, ReopenOutcome, close_new_tab, member_launch, open_place, reusable,
};

/// A member that passed launch's checks, with the tab its new session needs.
pub struct HandoffPlan<'r> {
    pub team: TeamName,
    pub member: Member,
    /// The id the new session starts with, for a harness that lets teamwire
    /// choose it. `None` for one that picks its own (Codex), whose
    /// session-start hook records it.
    pub new_session: Option<SessionId>,
    /// The argv of the new session, for `--dry-run`.
    pub argv: Vec<String>,
    /// The new session's first line.
    pub first_line: String,
    ready: Ready<'r>,
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
) -> Result<HandoffPlan<'r>, String> {
    let store_error = |e: teamwire_store::StoreError| format!("the store could not be read: {e}");
    let (team, member, options) = ctx
        .store
        .read(|tx| Ok((tx.team()?, tx.member(role)?, tx.run_options(role)?)))
        .map_err(store_error)?;
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
    let handoff = team_dir.join("reports").join(format!("{role}.handoff.md"));
    let first_line = format!(
        "This is a fresh session in your seat. Your brief is at {} and your previous session's \
         handoff document is at {}. Read both in full now, then carry on from where the \
         handoff document says.",
        brief.display(),
        handoff.display()
    );
    let request = SpawnRequest {
        name: Address::of(&team.name, role),
        title: member.title.clone(),
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
        ready,
    })
}

/// Opens the new tab, answers the trust dialog, records the handoff and
/// sends the first line. Run only once the old place is closed. On an error
/// the old session is still recorded, and the error says how to resume it.
pub fn open_handoff(ctx: &mut Ctx<'_>, plan: &HandoffPlan<'_>) -> Result<Handoff, String> {
    let role = &plan.member.role;
    let place = open_place(&plan.ready).map_err(|o| resume_hint(plan, &reason(o)))?;
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
            return Err(resume_hint(plan, &close_new_tab(&plan.ready, &place, &why)));
        }
        Err(e) => {
            let why = format!("the handoff could not be recorded: {e}");
            return Err(resume_hint(plan, &close_new_tab(&plan.ready, &place, &why)));
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

fn reason(outcome: ReopenOutcome) -> String {
    match outcome {
        ReopenOutcome::Reopened => "reopened".to_owned(),
        ReopenOutcome::Skipped(why) | ReopenOutcome::Refused(why) | ReopenOutcome::Failed(why) => {
            why
        }
    }
}

/// `why`, and how to get the member back: nothing recorded the handoff, so
/// the old session is still the member's, and reopen resumes it.
fn resume_hint(plan: &HandoffPlan<'_>, why: &str) -> String {
    let role = &plan.member.role;
    let old = plan
        .member
        .session_id
        .as_ref()
        .map_or_else(String::new, |s| format!(" {s}"));
    format!(
        "{role}: {why}. The old session{old} is still recorded, so `teamwire reopen {} --role \
         {role}` resumes it",
        plan.team
    )
}
