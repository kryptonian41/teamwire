//! The sanitiser. Every agent-supplied string that reaches another agent's
//! input or the event stream goes through `one_line` first (DEL-11, INV-20).

use std::fmt;

use serde::Serialize;

/// A string that has been through `one_line`: one line of printable
/// characters, capped. It is the only text `Host::submit` accepts, so an
/// unsanitised string cannot reach a session (DEL-11). It has no other
/// constructor and cannot be deserialised.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct OneLine(String);

impl OneLine {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for OneLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for OneLine {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The mark put at the end of a cut line.
pub const ELLIPSIS: char = '…';

/// Reduce `text` to one printable line of at most `max` characters.
///
/// - Line breaks, tabs and every other kind of white space become one space;
///   runs of them collapse, and the ends are trimmed.
/// - An ANSI escape sequence (ESC `[` ... final byte) is removed whole, and
///   every other control character is dropped.
/// - Characters that print nothing or reorder text are dropped: every
///   Unicode default-ignorable code point (soft hyphen, combining grapheme
///   joiner, zero-width and bidi marks, the byte-order mark, tag characters,
///   variation selectors, blank Hangul and Khmer fillers, the invisible
///   format controls U+2060-206F and the musical beam controls) and the
///   interlinear annotation marks U+FFF9-FFFB.
/// - A result longer than `max` characters is cut to `max - 1` characters
///   plus `…`.
///
/// `one_line(one_line(s, n), n) == one_line(s, n)` for every `s` and `n`.
pub fn one_line(text: &str, max: usize) -> OneLine {
    let mut out = String::with_capacity(text.len().min(max.saturating_mul(4)));
    let mut pending_space = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            skip_escape(&mut chars);
            continue;
        }
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if c.is_control() || is_invisible(c) {
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    OneLine(cut(out, max))
}

/// Skips the rest of an escape sequence after ESC: a CSI sequence
/// (`[`, parameters, one final byte in `@`..=`~`), or one following character.
fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek() {
        Some('[') => {
            chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
        Some(c) if !c.is_whitespace() && !c.is_control() => {
            chars.next();
        }
        Some(_) | None => {}
    }
}

/// Unicode's `Default_Ignorable_Code_Point` set, plus the interlinear
/// annotation marks U+FFF9-FFFB. Written out because the standard library has
/// no such table and a Unicode crate is not worth a dependency.
pub(crate) fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'..='\u{1160}'
            | '\u{17b4}'..='\u{17b5}'
            | '\u{180b}'..='\u{180f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{3164}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{feff}'
            | '\u{ffa0}'
            | '\u{fff0}'..='\u{fffb}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e0fff}'
    )
}

fn cut(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_breaks_become_one_space() {
        assert_eq!(one_line("a\r\n\tb", 80).as_str(), "a b");
    }

    #[test]
    fn escape_sequence_removed_whole() {
        assert_eq!(one_line("\u{1b}[31mred\u{1b}[0m", 80).as_str(), "red");
    }

    #[test]
    fn long_text_cut_with_ellipsis() {
        assert_eq!(one_line("abcdef", 4).as_str(), "abc…");
        assert_eq!(one_line("abcd", 4).as_str(), "abcd");
        assert_eq!(one_line("abcd", 0).as_str(), "");
    }

    #[test]
    fn invisible_marks_dropped() {
        assert_eq!(one_line("a\u{202e}b\u{200b}c", 80).as_str(), "abc");
    }
}
