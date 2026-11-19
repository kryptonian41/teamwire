//! The order of a handoff's steps: launch's checks, close's checks, close's
//! place close (the host refuses a second live agent of one name), then
//! launch's open, trust, record and first line. `teamwire-close` and
//! `teamwire-launch` may not depend on each other, so the close steps come in
//! as a `PlaceCloser` that the CLI fills with close's functions.
//!
//! Never: close the old place after a refusal; open a new session when the
//! old place's close failed.

use teamwire_core::{Address, Member, Role, TeamName};
use teamwire_store::Ctx;

use crate::handoff::{HandoffError, open_handoff, plan_handoff};

/// The close half of a handoff.
#[derive(Debug, Clone, Copy)]
pub struct PlaceCloser {
    /// Why the member's place must not close now; empty when it may.
    pub refusals: fn(&Ctx<'_>, &Member) -> Vec<String>,
    /// Closes the member's place and waits for its processes to end. `Ok`
    /// names what happened, `None` when no place is recorded; an error means
    /// the old session may still run.
    pub close: fn(&Ctx<'_>, &TeamName, &Member) -> Result<Option<String>, String>,
}

/// One thing a handoff tells the user, in the order it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Line(String),
    Warning(String),
}

/// Hands `role` off to a fresh session in its seat, step by step, and says
/// each step as it is done. A dry run says what it would do and writes
/// nothing. `Refused` and `Store` touched nothing; `Partial` comes after the
/// old place's close began.
pub fn hand_off(
    ctx: &mut Ctx<'_>,
    role: &Role,
    closer: PlaceCloser,
    dry_run: bool,
    say: &mut dyn FnMut(Said),
) -> Result<(), HandoffError> {
    let plan = plan_handoff(ctx, role)?;
    let refusals = (closer.refusals)(ctx, &plan.member);
    if !refusals.is_empty() {
        return Err(HandoffError::Refused(refusals.join("; ")));
    }
    say(Said::Line(format!(
        "{role}: started, idle and committed, so it can hand off"
    )));
    let old = plan.member.place.as_ref().map_or_else(
        || "no recorded place".to_owned(),
        |p| format!("place {}", p.id),
    );
    let session = plan.new_session.as_ref().map_or_else(
        || "the session it picks".to_owned(),
        |s| format!("session {s}"),
    );
    for w in &plan.warnings {
        say(Said::Warning(format!("warning: {w}")));
    }
    if dry_run {
        say(Said::Line(format!("{role}: would close {old}")));
        say(Said::Line(format!(
            "{role}: would open {} in {} with {session}: {}",
            Address::of(&plan.team, role),
            plan.member.cwd.display(),
            plan.argv.join(" ")
        )));
        say(Said::Line(format!(
            "{role}: would answer the trust dialog, record the handoff and send: {}",
            plan.first_line
        )));
        say(Said::Line(
            "dry run: nothing was closed, opened or written".to_owned(),
        ));
        return Ok(());
    }
    match (closer.close)(ctx, &plan.team, &plan.member) {
        Ok(Some(said)) => say(Said::Line(said)),
        Ok(None) => say(Said::Line(format!(
            "{role}: no recorded place, nothing to close"
        ))),
        Err(e) => {
            return Err(HandoffError::Partial(format!(
                "{e}. No new session was opened. The old session is still recorded, so once it \
                 has ended, `teamwire reopen {} --role {role}` resumes it",
                plan.team
            )));
        }
    }
    let done = open_handoff(ctx, &plan).map_err(HandoffError::Partial)?;
    say(Said::Line(format!(
        "{role}: opened {session} in place {}, trust step done",
        done.place.id
    )));
    say(Said::Line(format!(
        "{role}: handoff recorded, generation {}",
        done.generation
    )));
    for w in &done.warnings {
        say(Said::Warning(format!("warning: {w}")));
    }
    if done.warnings.is_empty() {
        say(Said::Line(format!(
            "{role}: first line sent, pointing at the brief and the handoff document"
        )));
    }
    Ok(())
}
