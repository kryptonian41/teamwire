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

struct Line<'a> {
    text: &'a str,
    /// Inside a code fence (or the fence itself): never a heading or a label.
    fenced: bool,
}

/// Every block in `report`, in file order.
pub fn find_blocks(report: &str) -> Vec<Block> {
    let mut in_fence = false;
    let lines: Vec<Line<'_>> = report
        .lines()
        .map(|text| {
            let fence = text.trim_start().starts_with("```");
            let was = in_fence;
            if fence {
                in_fence = !in_fence;
            }
            Line {
                text,
                fenced: was || fence,
            }
        })
        .collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        let Some(start) = Some(line)
            .filter(|l| !l.fenced)
            .and_then(|l| start_of(l.text))
        else {
            i += 1;
            continue;
        };
        let end = match start.level {
            Some(level) => heading_end(&lines, i, level),
            None => label_end(&lines, i, start.inline.is_some()),
        };
        let mut body: Vec<&str> = start.inline.into_iter().collect();
        body.extend(lines.iter().take(end).skip(i + 1).map(|l| l.text));
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
fn heading_end(lines: &[Line<'_>], from: usize, level: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(from + 1)
        .find(|(_, l)| !l.fenced && heading_level(l.text).is_some_and(|n| n <= level))
        .map_or(lines.len(), |(j, _)| j)
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
fn label_end(lines: &[Line<'_>], from: usize, has_inline: bool) -> usize {
    let mut seen_list = false;
    let mut prose_allowed = !has_inline;
    for (j, l) in lines.iter().enumerate().skip(from + 1) {
        if l.fenced {
            continue;
        }
        let line = l.text;
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
