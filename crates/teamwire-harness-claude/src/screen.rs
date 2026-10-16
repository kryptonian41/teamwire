//! Reading Claude Code's screen: is it waiting for input, or showing a
//! dialog? The answer decides whether a host may type into the session, so
//! `InputPrompt` needs positive evidence and nothing else is trusted.

use teamwire_core::ScreenState;

/// Text of the plain folder-trust dialog.
pub(crate) const PLAIN_TRUST_MARKERS: [&str; 2] =
    ["Yes, I trust this folder", "Is this a project you created"];

/// Text of the stronger dialog, which also grants what the folder's own
/// settings declare. Claude Code 2.1.284 prints "pre-approves" for
/// permissions.
pub(crate) const ESCALATION_MARKERS: [&str; 5] = [
    "will apply without asking",
    "runs commands to mint",
    "headersHelper",
    "This folder adds",
    "pre-approves",
];

/// Text a permission or confirmation dialog shows.
const OTHER_DIALOG_MARKERS: [&str; 6] = [
    "Do you want to proceed?",
    "Do you want to make this edit",
    "Do you want to create",
    "Do you want to allow",
    "Enter to confirm",
    "Esc to cancel",
];

/// Lines under the input box that only an idle session prints.
const FOOTER_MARKERS: [&str; 4] = [
    "? for shortcuts",
    "shift+tab to cycle",
    "bypass permissions on",
    "auto-accept edits on",
];

/// Text only a working session prints in its footer or status line.
const BUSY_MARKERS: [&str; 1] = ["esc to interrupt"];

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

/// What the last pointer line of the screen says, such as `No, exit`. A
/// dialog redraws its pointer as the selection moves, and the last one is the
/// current selection.
pub(crate) fn last_pointer_text(screen: &str) -> Option<String> {
    screen.lines().rev().find_map(pointer_rest).map(squash)
}

/// The non-empty lines at the end of the screen that can belong to the
/// input box, its footer, or a dialog drawn over them.
fn last_lines(tail: &str) -> Vec<&str> {
    let lines: Vec<&str> = tail.lines().filter(|l| !inner(l).is_empty()).collect();
    let start = lines.len().saturating_sub(BOX_LINES);
    lines.get(start..).unwrap_or_default().to_vec()
}

/// The selected choice of a dialog, such as `❯ 1. Yes`. An unselected choice
/// (`2. No`) is not counted: a numbered list in Claude's own reply looks the
/// same, and only the selection pointer is something a reply never prints.
fn is_selected_option(line: &str) -> bool {
    let Some(rest) = pointer_rest(line) else {
        return false;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest.get(digits..).is_some_and(|r| r.starts_with(". "))
}

/// A line of Claude's own reply or tool header, which starts with `⏺`.
/// Marker text on such a line is something Claude said, not a dialog.
fn is_transcript_line(line: &str) -> bool {
    inner(line).starts_with('⏺')
}

/// Claude keeps its input box and footer on screen while a dialog is open,
/// so a pointer line and a footer prove nothing when a dialog marker sits in
/// the same few lines. A false `Dialog` only delays a delivery; a false
/// `InputPrompt` types into a dialog.
fn dialog_near_the_end(last: &[&str]) -> bool {
    let drawn: Vec<&str> = last
        .iter()
        .copied()
        .filter(|l| !is_transcript_line(l))
        .collect();
    let text = squash(&drawn.join("\n"));
    has_any(&text, &PLAIN_TRUST_MARKERS)
        || has_any(&text, &ESCALATION_MARKERS)
        || has_any(&text, &OTHER_DIALOG_MARKERS)
        || last.iter().any(|l| is_selected_option(l))
}

/// A line of box-drawing dashes that closes Claude's input box.
fn is_rule(line: &str) -> bool {
    let line = inner(line);
    line.chars().count() >= 3 && line.chars().all(|c| "─━".contains(c))
}

/// The box Claude draws today: the pointer line, a rule, then the status and
/// footer lines. The footer text changes with the mode and with what runs in
/// the background, so the shape is the evidence, not the words. A working
/// session prints `esc to interrupt` and is not ready to be typed into.
fn is_ruled_box(last: &[&str]) -> bool {
    let busy = last
        .iter()
        .filter(|l| !is_transcript_line(l))
        .any(|l| BUSY_MARKERS.iter().any(|m| l.contains(m)));
    !busy
        && last.iter().enumerate().any(|(i, line)| {
            pointer_rest(line).is_some()
                && last.get(i + 1).is_some_and(|rule| is_rule(rule))
                && i + 2 < last.len()
        })
}

fn at_input_prompt(last: &[&str]) -> bool {
    // An empty pointer line as the last thing on screen.
    if last.last().and_then(|l| pointer_rest(l)) == Some("") {
        return true;
    }
    if is_ruled_box(last) {
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
    let last = last_lines(tail);
    if dialog_near_the_end(&last) {
        return ScreenState::Dialog;
    }
    if at_input_prompt(&last) {
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
