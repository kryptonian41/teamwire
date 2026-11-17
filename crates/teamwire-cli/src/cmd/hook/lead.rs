//! The lead's hooks: the Stop hook (ADR 0018 point 5) and the tool hook
//! (ADR 0035). Each runs in the lead's own session on every harness that can
//! lead, reads stores, writes no store, and gives up inside one time bound.
//! The only files they write are the two empty markers of the lead's own
//! handoff (ADR 0051), outside every store.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use teamwire_core::harness::WatchStart;
use teamwire_core::{Harness, HarnessId, TeamName};
use teamwire_store::Store;

use super::{STDIN_LIMIT, tool_note_reply, within};
use crate::Global;
use crate::cmd::shared::{carries_member_identity, state_home};
use crate::wiring::{Wiring, claude_harness_id, lead_handoff_setting};

/// The lead's hooks run on every stop, and on most tool calls, of every
/// session where the lead's setup is on, so a slow disk or a locked store
/// must not hold the session.
const LEAD_HOOK_BUDGET: Duration = Duration::from_secs(5);

/// The lead's Stop hook (ADR 0018 point 5): hold the stop once, naming each
/// team that has working members and no running watch, each AFK team that
/// nothing will wake, and each item never typed into the lead's pane (ADR
/// 0043), and the lead's own handoff once after its note (ADR 0051). It
/// reads stores and writes no store.
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
    let (teams, afk_notes, undelivered, handoff) = within(LEAD_HOOK_BUDGET, move || {
        let teams: Vec<_> =
            teamwire_report::teams_without_watch(&at, &turn.session, &*platform, &clock)
                .into_iter()
                .map(|team| {
                    let lead = lead_harness_of(&at, &team);
                    (team, lead)
                })
                .collect();
        let setting = lead_handoff_setting(&at, &|key| std::env::var(key).ok());
        (
            teams,
            teamwire_report::afk_hold_notes(&at, &turn.session, &*platform, &clock),
            teamwire_report::undelivered_to_lead(&at, &turn.session, &*platform, &clock),
            teamwire_report::lead_handoff_hold(&at, &turn.session, &*platform, &setting),
        )
    })?;
    let mut note: Vec<String> = teams
        .iter()
        .map(|(team, lead)| {
            teamwire_report::watch_missing_note(team, lead_watch_start(lead.as_ref(), wiring))
        })
        .collect();
    note.extend(afk_notes);
    note.extend(undelivered);
    note.extend(handoff);
    if note.is_empty() {
        return None;
    }
    Some(harness.hold_reply(&note.join("\n")))
}

/// The lead's tool hook (ADR 0035): a note beside a tool's result when the
/// lead did work that a member of its open team should do, and the items
/// never typed into the lead's pane (ADR 0043). The note never blocks the
/// call.
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
    let id = lead_harness_id(harness)?;
    let call = wiring
        .registry
        .harness(&id)
        .ok()?
        .parse_tool_call(&input)
        .ok()?;
    let platform = wiring.platform.clone();
    let clock = wiring.clock;
    let registry = Arc::clone(&wiring.registry);
    let at = home.to_path_buf();
    // The undelivered note is read on every call, not only for member work:
    // the call that frees a blocked pane is often a dialog's answer. A
    // subagent's note would reach the subagent, not the lead. The handoff
    // note reads the lead's transcript, so it too stays inside the bound.
    let (work, undelivered, handoff) = within(LEAD_HOOK_BUDGET, move || {
        let undelivered = if call.subagent {
            Vec::new()
        } else {
            teamwire_report::undelivered_to_lead(&at, &call.session, &*platform, &clock)
        };
        let setting = lead_handoff_setting(&at, &|key| std::env::var(key).ok());
        let handoff = registry.harness(&id).ok().and_then(|lead| {
            teamwire_report::lead_handoff_note(&at, &call, lead, &*platform, &setting)
        });
        (
            teamwire_report::lead_tool_note(&at, &call),
            undelivered,
            handoff,
        )
    })?;
    let note: Vec<String> = work.into_iter().chain(undelivered).chain(handoff).collect();
    if note.is_empty() {
        return None;
    }
    tool_note_reply(&note.join("\n"))
}

/// The harness that parses a lead hook's input: the one `--harness` names,
/// else Claude Code, whose plugin wrote the Stop hook before the flag existed.
fn lead_harness<'w>(harness: Option<&str>, wiring: &'w Wiring) -> Option<&'w dyn Harness> {
    wiring.registry.harness(&lead_harness_id(harness)?).ok()
}

/// The id `lead_harness` looks up.
fn lead_harness_id(harness: Option<&str>) -> Option<HarnessId> {
    match harness {
        Some(id) => HarnessId::parse(id).ok(),
        None => Some(claude_harness_id()),
    }
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
