//! Rust source with its comments, string literals and char literals blanked.
//!
//! Never: change the number of lines or the column of any code character, so
//! a line number found in the result is the line number in the file.
//! Invariants: every blanked character becomes a space, except a newline,
//! which stays.

/// `text` with the inside of every comment, string and char literal replaced
/// by spaces. A lifetime such as `'a` is code and stays.
pub(crate) fn code_only(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i = blank_block_comment(&chars, i, &mut out);
        } else if let Some(hashes) = raw_string_hashes(&chars, i) {
            i = blank_raw_string(&chars, i, hashes, &mut out);
        } else if c == '"' {
            i = blank_string(&chars, i, &mut out);
        } else if c == '\'' && is_char_literal(&chars, i) {
            i = blank_char(&chars, i, &mut out);
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn blank(c: char, out: &mut String) {
    out.push(if c == '\n' { '\n' } else { ' ' });
}

/// Block comments nest in Rust.
fn blank_block_comment(chars: &[char], start: usize, out: &mut String) -> usize {
    let mut depth = 0_usize;
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            depth += 1;
            out.push_str("  ");
            i += 2;
        } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
            depth -= 1;
            out.push_str("  ");
            i += 2;
            if depth == 0 {
                break;
            }
        } else {
            blank(chars[i], out);
            i += 1;
        }
    }
    i
}

/// The number of `#` of a raw string that opens at `i` (`r"`, `r#"`, `br"`),
/// or `None` when no raw string opens there.
fn raw_string_hashes(chars: &[char], i: usize) -> Option<usize> {
    if chars[i] != 'r' {
        return None;
    }
    let before = i.checked_sub(1).map(|p| chars[p]);
    let before = if before == Some('b') {
        i.checked_sub(2).map(|p| chars[p])
    } else {
        before
    };
    if before.is_some_and(|b| b.is_alphanumeric() || b == '_') {
        return None;
    }
    let hashes = chars[i + 1..].iter().take_while(|c| **c == '#').count();
    (chars.get(i + 1 + hashes) == Some(&'"')).then_some(hashes)
}

fn blank_raw_string(chars: &[char], start: usize, hashes: usize, out: &mut String) -> usize {
    let mut i = start + 2 + hashes;
    out.push_str(&" ".repeat(2 + hashes));
    while i < chars.len() {
        let closes = chars[i] == '"'
            && chars[i + 1..].iter().take_while(|c| **c == '#').count() >= hashes;
        if closes {
            out.push_str(&" ".repeat(1 + hashes));
            return i + 1 + hashes;
        }
        blank(chars[i], out);
        i += 1;
    }
    i
}

fn blank_string(chars: &[char], start: usize, out: &mut String) -> usize {
    out.push(' ');
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                out.push(' ');
                if let Some(escaped) = chars.get(i + 1) {
                    blank(*escaped, out);
                }
                i += 2;
            }
            '"' => {
                out.push(' ');
                return i + 1;
            }
            c => {
                blank(c, out);
                i += 1;
            }
        }
    }
    i
}

/// `'x'` and `'\n'` are char literals; `'a` followed by anything else is a
/// lifetime or a label.
fn is_char_literal(chars: &[char], i: usize) -> bool {
    match chars.get(i + 1) {
        Some('\\') => true,
        Some(_) => chars.get(i + 2) == Some(&'\''),
        None => false,
    }
}

fn blank_char(chars: &[char], start: usize, out: &mut String) -> usize {
    out.push(' ');
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                out.push_str("  ");
                i += 2;
            }
            '\'' => {
                out.push(' ');
                return i + 1;
            }
            c => {
                blank(c, out);
                i += 1;
            }
        }
    }
    i
}

#[cfg(test)]
mod tests {
    use super::code_only;

    #[test]
    fn comments_and_literals_are_blanked_and_lines_are_kept() {
        let src = "let a = \"x{\"; // y{\nlet b = '{'; /* z{\n nested /* { */ */ let c = r#\"q\"{\"#;\n";
        let out = code_only(src);
        assert_eq!(out.lines().count(), src.lines().count());
        assert!(!out.contains('{'), "{out}");
        assert!(out.contains("let a =") && out.contains("let b =") && out.contains("let c ="));
    }

    #[test]
    fn a_lifetime_is_code_and_an_escaped_quote_stays_inside_its_string() {
        let out = code_only("fn f<'a>(x: &'a str) { \"\\\"{\" }");
        assert!(out.contains("<'a>") && out.contains("&'a str"), "{out}");
        assert_eq!(out.matches('{').count(), 1, "{out}");
    }
}
