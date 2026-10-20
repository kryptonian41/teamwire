//! The line edits launch and close make to a repository's `info/exclude`,
//! as pure functions on its bytes. Both crates read, lock and write the file
//! themselves; only the edit is shared, so the two cannot disagree on what a
//! line is.

/// `bytes` with each entry appended unless a line already names it (with or
/// without a leading `/`). Existing lines are never rewritten or reordered.
/// Also returns the entries that were added; with none, the bytes are
/// returned unchanged.
pub fn add(bytes: &[u8], entries: &[String]) -> (Vec<u8>, Vec<String>) {
    let mut out = bytes.to_vec();
    let mut added: Vec<String> = Vec::new();
    let mut tail = Vec::new();
    for entry in entries {
        let bare = entry.strip_prefix('/').unwrap_or(entry).as_bytes();
        let present = bytes.split(|b| *b == b'\n').any(|l| {
            let l = l.trim_ascii();
            l.strip_prefix(b"/").unwrap_or(l) == bare
        });
        if !present && !added.contains(entry) {
            tail.extend_from_slice(entry.as_bytes());
            tail.push(b'\n');
            added.push(entry.clone());
        }
    }
    if added.is_empty() {
        return (out, added);
    }
    if !out.is_empty() && !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    out.extend_from_slice(&tail);
    (out, added)
}

/// `bytes` without `lines`, one occurrence each; every other line stays as
/// it was. A line two launches both added is still there once after one of
/// them is taken back.
pub fn remove(bytes: &[u8], lines: &[String]) -> Vec<u8> {
    let mut pending: Vec<&[u8]> = lines.iter().map(String::as_bytes).collect();
    let mut kept: Vec<&[u8]> = Vec::new();
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        let bare = line.strip_suffix(b"\n").unwrap_or(line);
        match pending.iter().position(|p| *p == bare) {
            Some(i) => {
                pending.swap_remove(i);
            }
            None => kept.push(line),
        }
    }
    kept.concat()
}

/// A path as an `info/exclude` line: anchored at the root, with glob
/// characters, a leading `#` or `!`, and a trailing space escaped.
pub fn anchored(path: &str) -> String {
    let mut line = String::from("/");
    for (i, c) in path.chars().enumerate() {
        if matches!(c, '*' | '?' | '[' | '\\') || (i == 0 && matches!(c, '#' | '!')) {
            line.push('\\');
        }
        line.push(c);
    }
    if line.ends_with(' ') {
        line.insert(line.len() - 1, '\\');
    }
    line
}

/// The path an `anchored` line names, relative to the repository root.
/// `None` for a line `anchored` cannot have written: close then cannot tell
/// whether a worktree still needs it.
pub fn path_of(line: &str) -> Option<String> {
    let rest = line.strip_prefix('/')?;
    let mut path = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => path.push(chars.next()?),
            '*' | '?' | '[' => return None,
            c => path.push(c),
        }
    }
    (!path.is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn add_appends_only_what_no_line_names() {
        let (out, added) = add(b"*.log\nTASK.md", &owned(&["/TASK.md", "/a", "/a"]));
        assert_eq!(out, b"*.log\nTASK.md\n/a\n");
        assert_eq!(added, ["/a"]);
    }

    #[test]
    fn add_with_nothing_new_returns_the_bytes_unchanged() {
        let (out, added) = add(b"/a", &owned(&["a"]));
        assert_eq!(out, b"/a");
        assert!(added.is_empty());
    }

    #[test]
    fn remove_takes_each_line_once_and_keeps_the_rest_in_order() {
        let out = remove(b"x\n/a\ny\n/a\n/b", &owned(&["/a", "/b", "/c"]));
        assert_eq!(out, b"x\ny\n/a\n");
    }

    #[test]
    fn remove_undoes_add() {
        let before = b"# mine\n*.tmp\n";
        let (after, added) = add(before, &owned(&["/TASK.md", "/node_modules/x"]));
        assert_eq!(remove(&after, &added), before);
    }

    #[test]
    fn path_of_reads_back_what_anchored_wrote() {
        for path in [
            "TASK.md",
            "a b/c ",
            "#x",
            "!y",
            "w*[?]\\z",
            "dir/!not-first",
        ] {
            assert_eq!(path_of(&anchored(path)).as_deref(), Some(path), "{path}");
        }
    }

    #[test]
    fn path_of_refuses_a_glob_or_an_unanchored_line() {
        assert_eq!(path_of("TASK.md"), None);
        assert_eq!(path_of("/*.log"), None);
        assert_eq!(path_of("/"), None);
        assert_eq!(path_of("/x\\"), None);
    }
}
