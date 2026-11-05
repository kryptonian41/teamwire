//! The lead's hooks: the Stop hook (ADR 0018 point 5) and the tool hook
//! (ADR 0035). Each runs in the lead's own session on every harness that can
//! lead, reads stores, writes nothing, and gives up inside one time bound.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use teamwire_core::harness::WatchStart;
use teamwire_core::{Harness, HarnessId, TeamName};
use teamwire_store::Store;

use super::{STDIN_LIMIT, tool_note_reply, within};
use crate::Global;
use crate::cmd::shared::{carries_member_identity, state_home};
use crate::wiring::{Wiring, claude_harness_id};

/// The lead's hooks run on every stop, and on most tool calls, of every
/// session where the lead's setup is on, so a slow disk or a locked store
/// must not hold the session.
const LEAD_HOOK_BUDGET: Duration = Duration::from_secs(5);

/// The lead's Stop hook (ADR 0018 point 5): hold the stop once, naming each
/// team that has working members and no running watch, and each AFK team that
/// nothing will wake. It reads stores and writes nothing.
pub(super) fn lead_stop_hook(
    harness: Option<&str>,
    global: &Global,
    wiring: &Wiring,
) -> Option<Vec<u8>> {
    // A member never runs this hook; the plugin is also switched off for it.
    if carries_member_identity() {
        return None;
    }
    let home = state_home(global)?;
    lead_stop_in(
        harness,
        &home,
        wiring,
        &mut std::io::stdin().lock().take(STDIN_LIMIT),
    )
}

pub(super) fn lead_stop_in(
    harness: Option<&str>,
    home: &Path,
    wiring: &Wiring,
    stdin: &mut dyn Read,
) -> Option<Vec<u8>> {
    let mut input = Vec::new();
    stdin.read_to_end(&mut input).ok()?;
    let harness = lead_harness(harness, wiring)?;
    let turn = harness.parse_turn_end(&input).ok()?;
    if turn.subagent || turn.hold_already_active {
        return None;
    }
    let platform = wiring.platform.clone();
    let clock = wiring.clock;
    let at = home.to_path_buf();
    // Every store read, the lead harness's included, stays inside the bound.
    let (teams, afk_notes) = within(LEAD_HOOK_BUDGET, move || {
        let teams: Vec<_> =
            teamwire_report::teams_without_watch(&at, &turn.session, &*platform, &clock)
                .into_iter()
                .map(|team| {
                    let lead = lead_harness_of(&at, &team);
                    (team, lead)
                })
                .collect();
        (
            teams,
            teamwire_report::afk_hold_notes(&at, &turn.session, &*platform, &clock),
        )
    })?;
    let mut note: Vec<String> = teams
        .iter()
        .map(|(team, lead)| {
            teamwire_report::watch_missing_note(team, lead_watch_start(lead.as_ref(), wiring))
        })
        .collect();
    note.extend(afk_notes);
    if note.is_empty() {
        return None;
    }
    Some(harness.hold_reply(&note.join("\n")))
}

/// The lead's tool hook (ADR 0035): a note beside a tool's result when the
/// lead did work that a member of its open team should do. The note never
/// blocks the call.
pub(super) fn lead_tool_hook(
    harness: Option<&str>,
    global: &Global,
    wiring: &Wiring,
) -> Option<Vec<u8>> {
    if carries_member_identity() {
        return None;
    }
    let home = state_home(global)?;
    lead_tool_in(
        harness,
        &home,
        wiring,
        &mut std::io::stdin().lock().take(STDIN_LIMIT),
    )
}

pub(super) fn lead_tool_in(
    harness: Option<&str>,
    home: &Path,
    wiring: &Wiring,
    stdin: &mut dyn Read,
) -> Option<Vec<u8>> {
    let mut input = Vec::new();
    stdin.read_to_end(&mut input).ok()?;
    let call = lead_harness(harness, wiring)?
        .parse_tool_call(&input)
        .ok()?;
    let at = home.to_path_buf();
    let note = within(LEAD_HOOK_BUDGET, move || {
        teamwire_report::lead_tool_note(&at, &call)
    })??;
    tool_note_reply(&note)
}

/// The harness that parses a lead hook's input: the one `--harness` names,
/// else Claude Code, whose plugin wrote the Stop hook before the flag existed.
fn lead_harness<'w>(harness: Option<&str>, wiring: &'w Wiring) -> Option<&'w dyn Harness> {
    let id = match harness {
        Some(id) => HarnessId::parse(id).ok()?,
        None => claude_harness_id(),
    };
    wiring.registry.harness(&id).ok()
}

/// The team's recorded lead harness, or `None` when the team cannot be read.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub(super) fn lead_harness_of(home: &Path, team: &TeamName) -> Option<HarnessId> {
    Store::open_read_only(home, team)
        .ok()
        .and_then(|store| store.read(|tx| tx.team()).ok().flatten())
        .map(|row| row.lead_harness)
}

/// How the lead harness starts a watch. A team that could not be read, or a
/// harness this binary does not have, gets the Monitor wording, which is the
/// older one.
pub(super) fn lead_watch_start(lead: Option<&HarnessId>, wiring: &Wiring) -> WatchStart {
    lead.and_then(|id| wiring.registry.harness(id).ok())
        .map_or(WatchStart::MonitoredCommand, |harness| {
            harness.capabilities().watch_start
        })
}
