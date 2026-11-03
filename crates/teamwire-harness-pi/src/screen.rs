//! Reading pi's screen: is it waiting for input, or showing a dialog? The
//! answer decides whether a host may type into the session, so `InputPrompt`
//! needs positive evidence and nothing else is trusted.
//!
//! pi 1.4.0 draws its input box as plain text between two rules of `─`, with
//! the footer under it: the folder, then a stats line whose context part
//! reads `0.0%/1.0M` (or `?/1.0M` just after a compaction). While a run
//! works, the status sits inside the top rule: `── ⠋ Working ───`. A dialog
//! or selector takes the box's place between the rules and prints a key hint
//! such as `↑↓ navigate  enter select  escape cancel`, with `→ ` on the
//! chosen option. `tests/screens/` holds screens captured from pi 1.4.0.

use teamwire_core::screen::{squash, visible};
use teamwire_core::{OneLine, ScreenState};

/// The project-trust dialog (`trust-selector.js`).
pub(crate) const TRUST_MARKER: &str = "Trust project folder?";

/// How many non-empty lines at the end of the screen can belong to the box,
/// its footer, or a dialog drawn in its place. The trust dialog with its
/// rules and the footer is about 14.
const BOX_LINES: usize = 30;

/// The fewest `─` a rule has: the box spans the terminal's width.
const MIN_RULE: usize = 10;

/// A rule of the box: a line that starts and ends with `─` and holds many.
fn is_border(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('─') && t.ends_with('─') && t.chars().filter(|c| *c == '─').count() >= MIN_RULE
}

/// A rule with a status inside it, such as `── ⠋ Working ───`. The box's
/// own scroll marks, `↑ 3 more` and `↓ 3 more`, are not a status.
fn is_busy_border(line: &str) -> bool {
    let inside = squash(&line.replace('─', " "));
    let scroll = ["↑ ", "↓ "].iter().any(|arrow| {
        inside
            .strip_prefix(arrow)
            .and_then(|rest| rest.strip_suffix(" more"))
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    });
    !inside.is_empty() && !scroll
}

/// A line only a dialog prints: its key hint, its title, or its chosen option.
fn is_dialog_line(line: &str) -> bool {
    let s = squash(line);
    s.contains("↑↓ navigate")
        || s.contains(TRUST_MARKER)
        || (s.contains("enter ") && s.contains(" cancel"))
        || line.trim_start().starts_with("→ ")
}

/// The context part of the stats line, which only the footer prints.
fn is_footer_stats(line: &str) -> bool {
    line.contains("%/") || line.contains("?/")
}

/// The non-empty lines at the end of the screen.
fn last_lines(tail: &str) -> Vec<&str> {
    let lines: Vec<&str> = tail.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(BOX_LINES);
    lines.get(start..).unwrap_or_default().to_vec()
}

/// The last box on the screen: its top rule, the lines between the rules,
/// and the lines under it.
struct Boxed<'a> {
    top: &'a str,
    inside: Vec<&'a str>,
    below: Vec<&'a str>,
}

fn last_box<'a>(last: &[&'a str]) -> Option<Boxed<'a>> {
    let bottom = last.iter().rposition(|l| is_border(l))?;
    let top = last.get(..bottom)?.iter().rposition(|l| is_border(l))?;
    Some(Boxed {
        top: last.get(top)?,
        inside: last.get(top + 1..bottom)?.to_vec(),
        below: last.get(bottom + 1..)?.to_vec(),
    })
}

/// `tail` may carry style codes; dim text counts, since pi draws a dialog's
/// hint and the footer dim.
pub(crate) fn classify(tail: &str) -> ScreenState {
    let plain = visible(tail, true);
    let last = last_lines(&plain);
    let Some(b) = last_box(&last) else {
        return if last.iter().any(|l| is_dialog_line(l)) {
            ScreenState::Dialog
        } else {
            ScreenState::Unknown
        };
    };
    if b.inside.iter().any(|l| is_dialog_line(l)) {
        ScreenState::Dialog
    } else if !is_busy_border(b.top) && b.below.iter().any(|l| is_footer_stats(l)) {
        ScreenState::InputPrompt
    } else {
        ScreenState::Unknown
    }
}

/// The input box's text, squashed, when the screen shows the box. Dim text
/// is dropped: pi draws what a person types plain.
fn box_text(tail: &str) -> Option<String> {
    let plain = visible(tail, false);
    let last = last_lines(&plain);
    let b = last_box(&last)?;
    Some(squash(&b.inside.join("\n")))
}

/// The state delivery acts on: `classify`, except that a box holding text is
/// `Unknown`, so a line is never appended to what a person half typed.
pub(crate) fn delivery_state(tail: &str) -> ScreenState {
    let state = classify(tail);
    if state == ScreenState::InputPrompt && box_holds_text(tail) {
        ScreenState::Unknown
    } else {
        state
    }
}

/// Whether the input box holds text, such as a line a person half typed.
/// An empty box shows only the cursor, a space.
pub(crate) fn box_holds_text(tail: &str) -> bool {
    classify(tail) == ScreenState::InputPrompt && box_text(tail).is_some_and(|t| !t.is_empty())
}

/// Whether `line`, just submitted, still sits in the box, unsent. Used after
/// a submit to decide on one more Enter. pi wraps a long line, sometimes
/// inside a word, so the text is compared with all whitespace taken out.
pub(crate) fn line_pending(tail: &str, line: &OneLine) -> bool {
    let Some(text) = box_text(tail) else {
        return false;
    };
    let solid = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let wanted = solid(line.as_str());
    (!wanted.is_empty() && solid(&text).contains(&wanted)) || is_paste_of(&text, line)
}

/// pi's editor shows a paste of more than 1000 characters as
/// `[paste #1 1234 chars]` (`pi-tui` `editor.js`). The count is in UTF-16
/// units, JavaScript's string length; only a marker whose count is the
/// line's own length counts, so a person's other paste does not.
fn is_paste_of(text: &str, line: &OneLine) -> bool {
    let count = text
        .strip_prefix("[paste #")
        .and_then(|rest| rest.split_once(' '))
        .filter(|(id, _)| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
        .and_then(|(_, rest)| rest.strip_suffix(" chars]"))
        .and_then(|n| n.parse::<usize>().ok());
    count == Some(line.as_str().encode_utf16().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scroll_mark_in_the_top_rule_is_not_a_status() {
        assert!(!is_busy_border("──── ↑ 3 more ────────────"));
        assert!(is_busy_border("── ⠋ Working (escape to interrupt) ──────"));
        assert!(!is_busy_border("────────────────────"));
    }

    #[test]
    fn a_short_dash_run_is_not_a_rule() {
        assert!(!is_border("-- ── --"));
        assert!(is_border("  ──────────────  "));
    }
}
