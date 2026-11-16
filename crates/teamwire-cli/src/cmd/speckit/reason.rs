//! Why the text of a tracked file mentions a task path (ADR 0044 decision
//! 4, ADR 0048, ADR 0049): the whole-token matches of the exact path, a
//! partial path and a dotted module name.

/// Why `text`, the text of the tracked file `file`, mentions `path`: its
/// exact text, a dotted module name, or a partial path, each as a whole
/// token (FR-014). The exact path also counts right after a `/`, as in
/// `./apps/x.py` or `$ROOT/apps/x.py`. A partial path holds a `/` and ends
/// the task path, so `harness/tools.py` names `apps/ai/harness/tools.py`
/// and `other/tools.py` does not. The bare file name alone never counts
/// (ADR 0048): `lib.rs` or `SKILL.md` names too many files of a real
/// repository. A root file counts only through a relative link that
/// resolves to the root (ADR 0049 rule B).
pub(crate) fn text_reason(file: &str, text: &str, path: &str) -> Option<String> {
    if !path.contains('/') {
        return super::root::link(file, text, path).map(|link| format!("names path {link}"));
    }
    let after_slash = |before: &str| before.ends_with('/');
    if has_token_after(text, path, after_slash) {
        return Some(format!("names path {path}"));
    }
    if let Some(module) = dotted_names(path)
        .into_iter()
        .find(|m| super::module::named(text, m))
    {
        return Some(format!("names module {module}"));
    }
    partial_path(text, path).map(|tail| format!("names path {tail}"))
}

/// The first partial path of `path` in `text`: its file name right after a
/// `/`, where the words of the token before the name end `path` too. A
/// leading `./` is left off the partial path it returns.
fn partial_path(text: &str, path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let has_ext = name
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty());
    if !has_ext || name == path {
        return None;
    }
    text.match_indices(name).find_map(|(at, _)| {
        let head = text.get(..at)?.strip_suffix('/')?;
        // The words of the token before the name, back to its start.
        let start = head
            .char_indices()
            .rev()
            .find(|(_, c)| !joins(*c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let words = head.get(start..)?.trim_start_matches("./");
        let tail = format!("{words}/{name}");
        let ends_path = path == tail || path.ends_with(&format!("/{tail}"));
        let rest = text.get(at + name.len()..)?;
        (!words.is_empty() && ends_path && ends_token(rest)).then_some(tail)
    })
}

/// The dotted module names of `path`: every suffix of its parts, without
/// the extension, of at least two parts, each part a plain identifier.
/// `apps/ai/harness/tools.py` gives `harness.tools`, `ai.harness.tools` and
/// `apps.ai.harness.tools`. A single part, the bare stem, is never one.
/// Only a Python file has module names (ADR 0049 rule A): the dotted name
/// of `prompts/system.md` matched attribute access, not an import.
fn dotted_names(path: &str) -> Vec<String> {
    let Some(stem) = path.strip_suffix(".py") else {
        return Vec::new();
    };
    let parts: Vec<&str> = stem.split('/').collect();
    let plain =
        |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let mut out = Vec::new();
    for start in (0..parts.len().saturating_sub(1)).rev() {
        let tail = parts.get(start..).unwrap_or_default();
        if !tail.iter().all(plain) {
            break;
        }
        out.push(tail.join("."));
    }
    out
}

/// A character that continues a name or a path, so a match next to it is
/// part of a longer word.
pub(crate) fn joins(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.')
}

/// True when `needle` occurs in `text` as a whole token, or after a name
/// character when `also(text before the match)` holds.
fn has_token_after(text: &str, needle: &str, also: impl Fn(&str) -> bool) -> bool {
    token_starts(text, needle, also).next().is_some()
}

/// Where each match of `needle` in `text` starts, as a whole token. A `.`
/// after it ends a sentence, so it counts as a break unless a name
/// character follows. A match that a name character comes before also
/// counts when `also(text before the match)` holds.
pub(crate) fn token_starts<'a>(
    text: &'a str,
    needle: &'a str,
    also: impl Fn(&str) -> bool + 'a,
) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(needle)
        .filter(move |(at, _)| {
            let head = text.get(..*at).unwrap_or("");
            let before = head.chars().next_back();
            let rest = text.get(at + needle.len()..).unwrap_or("");
            let free_before = before.is_none_or(|c| !joins(c)) || also(head);
            !needle.is_empty() && free_before && ends_token(rest)
        })
        .map(|(at, _)| at)
}

/// True when `rest`, the text after a match, starts with a break. A `.`
/// counts as a break unless a name character follows it.
pub(crate) fn ends_token(rest: &str) -> bool {
    let mut after = rest.chars();
    match after.next() {
        None => true,
        Some('.') => after.next().is_none_or(|c| !c.is_ascii_alphanumeric()),
        Some(c) => !joins(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dotted_name_needs_two_plain_parts() {
        assert_eq!(
            dotted_names("apps/ai/harness/tools.py"),
            ["harness.tools", "ai.harness.tools", "apps.ai.harness.tools"]
        );
        assert!(dotted_names("crates/teamwire-cli/src/main.rs").is_empty());
        assert!(dotted_names("agents/x/prompts/system.md").is_empty());
        assert!(dotted_names("tools.py").is_empty());
        let whole = |text: &str| has_token_after(text, "harness.tools", |_| false);
        assert!(whole("uses harness.tools."));
        assert!(!whole("uses harness.tools.x"));
    }
}
