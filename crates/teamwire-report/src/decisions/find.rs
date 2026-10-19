//! Finds the "Decisions I made" blocks in a report. Members write the
//! heading in several forms (a heading, a label line, a bold line, a
//! numbered heading), so the finder follows what real reports do.

use serde::Serialize;

/// One "Decisions I made" block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Block {
    /// The heading or label as written, such as `## Decisions I made`.
    pub heading: String,
    /// The 1-based line of the heading in the report.
    pub line: usize,
    /// The block's body, without the heading. Text after a label on the
    /// same line starts the body.
    pub text: String,
}

const LABEL: &str = "decisions i made";

/// Every block in `report`, in file order.
pub fn find_blocks(report: &str) -> Vec<Block> {
    let lines: Vec<&str> = report.lines().collect();
    // A `#` line inside a code fence is not a heading.
    let mut in_fence = false;
    let fenced: Vec<bool> = lines
        .iter()
        .map(|l| {
            let fence = l.trim_start().starts_with("```");
            let was = in_fence;
            if fence {
                in_fence = !in_fence;
            }
            was || fence
        })
        .collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(start) = lines.get(i).filter(|_| !fenced[i]).and_then(|l| start_of(l)) else {
            i += 1;
            continue;
        };
        let end = match start.level {
            Some(level) => heading_end(&lines, &fenced, i, level),
            None => label_end(&lines, &fenced, i, start.inline.is_some()),
        };
        let mut body: Vec<&str> = start.inline.into_iter().collect();
        body.extend(lines.get(i + 1..end).unwrap_or_default());
        blocks.push(Block {
            heading: start.heading,
            line: i + 1,
            text: body.join("\n").trim().to_owned(),
        });
        i = end.max(i + 1);
    }
    blocks
}

struct Start<'a> {
    /// `Some(n)` for a heading of `n` `#`s.
    level: Option<usize>,
    heading: String,
    inline: Option<&'a str>,
}

/// Whether `line` opens a block: after any `#`s, a number such as `7.` and
/// `**`, it begins with the label. A mention mid-line is not a start.
fn start_of(line: &str) -> Option<Start<'_>> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    let level = (hashes > 0 && trimmed[hashes..].starts_with(' ')).then_some(hashes);
    let mut rest = if level.is_some() {
        &trimmed[hashes..]
    } else {
        trimmed
    }
    .trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(after) = rest[digits..].strip_prefix(['.', ')'])
    {
        rest = after.trim_start();
    }
    let unbold = rest.trim_start_matches('*');
    if !unbold.get(..LABEL.len())?.eq_ignore_ascii_case(LABEL) {
        return None;
    }
    if level.is_some() {
        return Some(Start {
            level,
            heading: trimmed.trim_end().to_owned(),
            inline: None,
        });
    }
    // The label ends at the first colon; what follows it is part of the body.
    match trimmed.find(':') {
        Some(colon) => {
            let mut cut = colon + 1;
            if trimmed[cut..].starts_with("**") {
                cut += 2;
            }
            let inline = trimmed[cut..].trim();
            Some(Start {
                level: None,
                heading: trimmed[..cut].to_owned(),
                inline: (!inline.is_empty()).then_some(inline),
            })
        }
        None => Some(Start {
            level: None,
            heading: trimmed.trim_end().to_owned(),
            inline: None,
        }),
    }
}

fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    (hashes > 0 && trimmed[hashes..].starts_with(' ')).then_some(hashes)
}

/// A heading block runs to the next heading of the same or a higher level.
fn heading_end(lines: &[&str], fenced: &[bool], from: usize, level: usize) -> usize {
    (from + 1..lines.len())
        .find(|&j| {
            !fenced[j]
                && lines
                    .get(j)
                    .and_then(|l| heading_level(l))
                    .is_some_and(|n| n <= level)
        })
        .unwrap_or(lines.len())
}

fn is_list_item(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with(['-', '*', '+']) && t[1..].starts_with(' ') {
        return true;
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && t[digits..].starts_with(['.', ')'])
}

/// Any other block runs to the next heading, the next bold label line, or the
/// first non-blank line that is neither a list item nor an indented
/// continuation. Prose right under a bare label (no list yet, no blank line)
/// belongs to the block.
fn label_end(lines: &[&str], fenced: &[bool], from: usize, has_inline: bool) -> usize {
    let mut seen_list = false;
    let mut prose_allowed = !has_inline;
    for j in from + 1..lines.len() {
        let line = lines.get(j).copied().unwrap_or_default();
        if fenced[j] {
            continue;
        }
        if line.trim().is_empty() {
            prose_allowed = false;
            continue;
        }
        if heading_level(line).is_some() || line.trim_start().starts_with("**") {
            return j;
        }
        if is_list_item(line) {
            seen_list = true;
            continue;
        }
        let indented = line.starts_with([' ', '\t']);
        if indented || (prose_allowed && !seen_list) {
            continue;
        }
        return j;
    }
    lines.len()
}

#[cfg(test)]
mod tests;
