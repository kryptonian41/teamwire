//! The order of a lead handoff's steps (ADR 0051): find the caller's led
//! teams, refuse before anything changes, open a pane beside the old lead
//! with a fresh lead session, prove it runs there, record it as each team's
//! lead, move the sleep hold, report, and close the old lead's pane last,
//! because this command runs inside that pane and ends with it.
//!
//! Never: hold a write transaction during a host call; roll back a team
//! that moved; close a pane this command did not open or did not locate
//! right before; end a process by signal.

use std::path::PathBuf;
use std::time::Duration;

use teamwire_core::{Clock, Config, HostError, Place, SessionId, TeamName};
use teamwire_deliver::locate_session;
use teamwire_store::{Ctx, Store};

use crate::handoff::HandoffError;
use crate::handoff_order::Said;
use crate::hold::ensure_hold;
use crate::lead_adopt::{adopt_all, moved_teams, wait_for_adoption};
use crate::lead_plan::{LeadCaller, LeadHandoffCtx, Planned, find_lead, plan};

const HOST_TIMEOUT: Duration = Duration::from_secs(30);

/// The longest wait for the new session to show in the new pane. The old
/// lead runs this command through its own shell tool, and Claude Code's Bash
/// tool stops a command after 120 seconds by default. A wait that ends by 90
/// seconds leaves time to close the new pane and print why before that.
const MAX_PROVE_WAIT: Duration = Duration::from_secs(90);

/// How long the handoff waits, and how often it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeadHandoffWaits {
    /// The pause between two looks.
    pub pause: Duration,
    /// The longest wait for the new session to show in the new pane.
    pub prove: Duration,
    /// The longest wait for a new lead that picks its own session id to
    /// adopt every team.
    pub adopt: Duration,
}

impl LeadHandoffWaits {
    /// Proving waits as long as launch waits for one agent
    /// (`Config::spawn_timeout_s`), but never past `MAX_PROVE_WAIT`;
    /// adopting waits 180 seconds.
    #[must_use]
    pub fn of(config: &Config) -> Self {
        Self {
            pause: Duration::from_millis(500),
            prove: Duration::from_secs(config.spawn_timeout_s).min(MAX_PROVE_WAIT),
            adopt: Duration::from_secs(180),
        }
    }
}

/// How the command was asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadHandoffOpts {
    /// The state directory the caller named (`--home` or `TEAMWIRE_HOME`),
    /// passed to the new lead as `TEAMWIRE_HOME`.
    pub home_env: Option<PathBuf>,
    /// Say what would happen, and change nothing.
    pub dry_run: bool,
    pub waits: LeadHandoffWaits,
}

/// Hands every open team the caller leads to a fresh lead session in a
/// pane beside the caller's, and says each step as it is done. `Refused`
/// and `Store` changed nothing. `Partial` says which teams moved and which
/// panes stay open.
pub fn lead_hand_off(
    ctx: &LeadHandoffCtx<'_>,
    caller: &LeadCaller<'_>,
    opts: &LeadHandoffOpts,
    say: &mut dyn FnMut(Said),
) -> Result<(), HandoffError> {
    let seat = find_lead(ctx, caller)?;
    let planned = plan(ctx, seat, opts.home_env.as_deref())?;
    for w in &planned.warnings {
        say(Said::Warning(format!("warning: {w}")));
    }
    if opts.dry_run {
        dry_run(&planned, say);
        return Ok(());
    }
    for flag in &planned.dropped {
        say(Said::Warning(format!(
            "warning: the old lead's flag {flag} is not passed to the new lead"
        )));
    }
    let timeout = Duration::from_secs(ctx.config.spawn_timeout_s);
    let new_place = planned
        .host
        .split_run(&planned.old_place, &planned.split, timeout)
        .map_err(|e| {
            HandoffError::Partial(format!(
                "the new pane could not be opened: {e}. Every team still has this session as \
                 its lead"
            ))
        })?;
    say(Said::Line(format!(
        "opened pane {} beside pane {}: {}",
        new_place.id,
        planned.old_place.id,
        planned.split.argv.first().map_or("", String::as_str)
    )));
    let adopted = if let Some(new) = &planned.new_session {
        prove(ctx, &planned, new, &new_place, opts.waits)?;
        say(Said::Line(format!(
            "session {new} runs in pane {}",
            new_place.id
        )));
        adopt_all(ctx, &planned, new, &new_place)
    } else {
        say(Said::Line(format!(
            "waiting up to {} s for the new lead to adopt each team",
            opts.waits.adopt.as_secs()
        )));
        wait_for_adoption(ctx, &planned, &new_place, opts.waits)
    };
    let moved = match adopted {
        Ok(moved) => moved,
        Err(e) => {
            // A team that moved before the failure must not keep a hold that
            // waits on the old lead's process, which ends with its pane.
            let mut warnings = Vec::new();
            for (team, session) in moved_teams(ctx, &planned) {
                hold(ctx, &planned, &team, &session, &mut warnings);
            }
            for w in warnings {
                say(Said::Warning(format!("warning: {w}")));
            }
            return Err(e);
        }
    };
    let mut warnings = Vec::new();
    for (team, session) in &moved {
        say(Said::Line(format!(
            "team {team}: lead is now session {session}"
        )));
        hold(ctx, &planned, team, session, &mut warnings);
    }
    for w in warnings {
        say(Said::Warning(format!("warning: {w}")));
    }
    let names: Vec<String> = moved.iter().map(|(t, _)| t.to_string()).collect();
    say(Said::Line(format!(
        "handed off {} to the new lead in pane {}; the handoff document is {}",
        names.join(", "),
        new_place.id,
        planned.files.document.display()
    )));
    say(Said::Line(format!(
        "session {} is no longer the lead; this pane closes now",
        planned.seat.session
    )));
    close_old(ctx, &planned, say);
    Ok(())
}

fn dry_run(planned: &Planned<'_>, say: &mut dyn FnMut(Said)) {
    let names: Vec<String> = planned
        .seat
        .names()
        .iter()
        .map(ToString::to_string)
        .collect();
    say(Said::Line(format!("teams: {}", names.join(", "))));
    say(Said::Line(format!(
        "old pane: {} (session {})",
        planned.old_place.id, planned.seat.session
    )));
    say(Said::Line(format!(
        "handoff document: {}",
        planned.files.document.display()
    )));
    say(Said::Line(format!(
        "would open a pane labelled \"{}\" in {}",
        planned.split.label,
        planned.split.cwd.display()
    )));
    say(Said::Line(format!(
        "new session: {}",
        planned.new_session.as_ref().map_or_else(
            || "the one its harness picks".to_owned(),
            ToString::to_string
        )
    )));
    say(Said::Line(format!(
        "command: {}",
        teamwire_core::LaunchSpec::shell_line(&planned.split.argv)
    )));
    for flag in &planned.dropped {
        say(Said::Line(format!("not passed on: {flag}")));
    }
    say(Said::Line(
        "dry run: no pane was opened and no team changed".to_owned(),
    ));
}

/// Looks with `look` until it says yes, at most `limit` long and at most
/// `limit / pause` times, pausing between looks. The count bounds a clock
/// that stands still in a test.
pub(crate) fn poll<T>(
    clock: &dyn Clock,
    waits: (Duration, Duration),
    mut look: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    let (pause, limit) = waits;
    let start = clock.monotonic();
    let tries = limit.as_millis().div_ceil(pause.as_millis().max(1)).max(1);
    let mut last = String::new();
    for _ in 0..tries {
        match look() {
            Ok(found) => return Ok(found),
            Err(why) => last = why,
        }
        if clock.monotonic().saturating_duration_since(start) >= limit {
            break;
        }
        std::thread::sleep(pause);
    }
    Err(last)
}

/// Step 5: the new session runs in the new pane, by both views of the
/// locator. On failure the new pane is closed and nothing changed.
fn prove(
    ctx: &LeadHandoffCtx<'_>,
    planned: &Planned<'_>,
    new: &SessionId,
    new_place: &Place,
    waits: LeadHandoffWaits,
) -> Result<(), HandoffError> {
    let found = poll(
        ctx.clock,
        (waits.pause, waits.prove),
        || match locate_session(ctx.platform, ctx.clock, new, planned.harness, planned.host) {
            Ok(place) if place == *new_place => Ok(()),
            Ok(place) => Err(format!("it was found in pane {}", place.id)),
            Err(e) => Err(format!("{}: {}", e.code, e.detail)),
        },
    );
    let Err(why) = found else {
        return Ok(());
    };
    let closed = planned.host.close_pane(new_place, HOST_TIMEOUT);
    Err(HandoffError::Partial(format!(
        "the new lead session {new} was not seen in pane {} within {} s ({why}). {} Every team \
         still has this session as its lead",
        new_place.id,
        waits.prove.as_secs(),
        closed_text(new_place, closed)
    )))
}

fn closed_text(place: &Place, closed: Result<(), HostError>) -> String {
    match closed {
        Ok(()) => format!("Pane {} was closed.", place.id),
        Err(e) => format!(
            "Pane {} could not be closed ({e}); close it by hand.",
            place.id
        ),
    }
}

/// Step 8: the sleep hold waits on the new lead's process.
fn hold(
    ctx: &LeadHandoffCtx<'_>,
    planned: &Planned<'_>,
    team: &TeamName,
    session: &SessionId,
    warnings: &mut Vec<String>,
) {
    let mut store = match Store::open_existing(ctx.home, team) {
        Ok(store) => store,
        Err(e) => {
            warnings.push(format!(
                "team {team}: the sleep hold was not moved, because the store could not be \
                 opened ({e})"
            ));
            return;
        }
    };
    let mut team_ctx = Ctx {
        home: ctx.home,
        store: &mut store,
        registry: ctx.registry,
        platform: ctx.platform,
        clock: ctx.clock,
        config: ctx.config,
    };
    let mut said = Vec::new();
    ensure_hold(
        &mut team_ctx,
        team,
        (&planned.seat.harness, Some(session)),
        &mut said,
    );
    warnings.extend(said.into_iter().map(|w| format!("team {team}: {w}")));
}

/// Step 10: the old session is located again, then its pane is closed.
fn close_old(ctx: &LeadHandoffCtx<'_>, planned: &Planned<'_>, say: &mut dyn FnMut(Said)) {
    let old = &planned.seat.session;
    let located = locate_session(ctx.platform, ctx.clock, old, planned.harness, planned.host)
        .map_err(|e| format!("{}: {}", e.code, e.detail))
        .and_then(|place| {
            if place == planned.old_place {
                Ok(place)
            } else {
                Err(format!("the session is now in pane {}", place.id))
            }
        });
    let closed = located.and_then(|place| {
        planned
            .host
            .close_pane(&place, HOST_TIMEOUT)
            .map_err(|e| e.to_string())
    });
    if let Err(why) = closed {
        say(Said::Warning(format!(
            "warning: this pane ({}) could not be closed ({why}). End this turn now; the user \
             can close the pane",
            planned.old_place.id
        )));
    }
}
