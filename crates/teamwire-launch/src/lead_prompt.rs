//! The first prompt of a new lead after a lead handoff (ADR 0051): one
//! paragraph that points the new lead at the handoff document. One function
//! builds it, and takes its harness words from `HarnessCaps`.

use std::fmt::Write as _;
use std::path::Path;

use teamwire_core::harness::WatchStart;
use teamwire_core::{HarnessCaps, SessionId};
use teamwire_deliver::{WatchText, watch_text};
use teamwire_store::TeamRow;

/// What the first prompt names.
pub(crate) struct FirstPrompt<'a> {
    pub(crate) old: &'a SessionId,
    pub(crate) teams: &'a [TeamRow],
    pub(crate) document: &'a Path,
    /// The new lead records itself as each team's lead first: its harness
    /// picks its own session id, or the user starts it by hand.
    pub(crate) adopt_first: bool,
}

/// The new lead's first prompt, one paragraph. Its harness words come from
/// `caps`: how to load the teamwire skill, and how to start a watch.
pub(crate) fn first_prompt(caps: &HarnessCaps, p: &FirstPrompt<'_>) -> String {
    let names: Vec<String> = p.teams.iter().map(|t| t.name.to_string()).collect();
    let teams = names.join(", ");
    let mut text = String::new();
    if p.adopt_first {
        write!(
            text,
            "First run `teamwire lead --adopt <team>` for each of these teams: {teams}. "
        )
        .ok();
    }
    write!(
        text,
        "You are a teamwire lead. You take over from lead session {}, which handed off because \
         its context was full. You now lead these teams: {teams}. {}",
        p.old, caps.load_skill
    )
    .ok();
    for team in p.teams.iter().filter(|t| t.afk_budget_ends_at.is_some()) {
        write!(
            text,
            " Team {} is an away run: load the teamwire:afk skill too, and follow its section \
             on taking over a run.",
            team.name
        )
        .ok();
    }
    let status: Vec<String> = names
        .iter()
        .map(|n| format!("`teamwire status {n}`"))
        .chain(["`teamwire lead --missed`".to_owned()])
        .collect();
    let watches: Vec<String> = p
        .teams
        .iter()
        .map(|t| {
            let text = watch_text(caps.watch_start, WatchText::Launch, &t.name);
            match caps.watch_start {
                WatchStart::ExtensionTool => text,
                WatchStart::MonitoredCommand => format!("`{text}`"),
            }
        })
        .collect();
    write!(
        text,
        " Then read {} in full. It assumes no prior conversation. Then run {}. Start a watch for \
         each team that has working members ({}). If the watch is refused because another watch \
         holds the team, wait 15 seconds and try once more. Then carry on from the document's \
         Next section. Do not plan the work again, and do not ask the user what the document \
         already answers.",
        p.document.display(),
        and_list(&status),
        watches.join("; ")
    )
    .ok();
    text
}

/// `a`, `a and b`, `a, b and c`.
fn and_list(items: &[String]) -> String {
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}
