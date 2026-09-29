//! Reading Claude Code's screen: is it waiting for input, or showing a
//! dialog? The answer decides whether a host may type into the session, so
//! `InputPrompt` needs positive evidence and nothing else is trusted.

use teamwire_core::ScreenState;

/// Text of the plain folder-trust dialog (`team-launch.py` `TRUST_MARKERS`).
pub(crate) const PLAIN_TRUST_MARKERS: [&str; 2] =
    ["Yes, I trust this folder", "Is this a project you created"];

/// Text of the stronger dialog, which also grants what the folder's own
/// settings declare (`team-launch.py` `TRUST_ESCALATION_MARKERS`, plus
/// "pre-approves", which Claude Code 2.1.284 prints for permissions).
pub(crate) const ESCALATION_MARKERS: [&str; 5] = [
    "will apply without asking",
    "runs commands to mint",
    "headersHelper",
    "This folder adds",
    "pre-approves",
];

/// Text a permission or confirmation dialog shows.
const OTHER_DIALOG_MARKERS: [&str; 5] = [
    "Do you want to proceed?",
    "Do you want to make this edit",
    "Do you want to create",
    "Do you want to allow",
    "Enter to confirm",
];

/// Lines under the input box that only an idle session prints.
const FOOTER_MARKERS: [&str; 4] = [
    "? for shortcuts",
    "shift+tab to cycle",
    "bypass permissions on",
    "auto-accept edits on",
];

/// How many non-empty lines at the end of the screen can belong to the
/// input box and its footer.
const BOX_LINES: usize = 8;

/// The text with each run of whitespace made one space, so a phrase that the
/// terminal wrapped onto two lines still matches.
pub(crate) fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn has_any(squashed: &str, markers: &[&str]) -> bool {
    markers.iter().any(|m| squashed.contains(m))
}

/// A line with its box-drawing border and outer spaces removed.
fn inner(line: &str) -> &str {
    line.trim_matches(|c: char| c.is_whitespace() || "│┃|".contains(c))
}

/// The pointer that starts Claude's input line: `❯` (U+276F), or the plain
/// `>` that older builds and the fake print.
fn pointer_rest(line: &str) -> Option<&str> {
    let line = inner(line);
    line.strip_prefix('❯')
        .or_else(|| line.strip_prefix('>'))
        .map(str::trim)
}

fn at_input_prompt(tail: &str) -> bool {
    let lines: Vec<&str> = tail.lines().filter(|l| !inner(l).is_empty()).collect();
    let start = lines.len().saturating_sub(BOX_LINES);
    let last = lines.get(start..).unwrap_or_default();
    // An empty pointer line as the last thing on screen.
    if last.last().and_then(|l| pointer_rest(l)) == Some("") {
        return true;
    }
    // A pointer line (empty, or holding a suggestion or typed text) with the
    // idle footer under it. A dialog's `❯ No, exit` has no such footer.
    last.iter().enumerate().any(|(i, line)| {
        pointer_rest(line).is_some()
            && last
                .iter()
                .skip(i + 1)
                .any(|below| FOOTER_MARKERS.iter().any(|f| below.contains(f)))
    })
}

pub(crate) fn classify(tail: &str) -> ScreenState {
    if at_input_prompt(tail) {
        return ScreenState::InputPrompt;
    }
    let squashed = squash(tail);
    if has_any(&squashed, &PLAIN_TRUST_MARKERS)
        || has_any(&squashed, &ESCALATION_MARKERS)
        || has_any(&squashed, &OTHER_DIALOG_MARKERS)
    {
        ScreenState::Dialog
    } else {
        ScreenState::Unknown
    }
}
