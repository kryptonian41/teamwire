//! The task lines of a spec-kit `tasks.md` (ADR 0044 decision 2): phases,
//! tasks, their story labels and the paths each task names.

/// One `## Phase <n>: <title>` heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Phase {
    pub(crate) number: String,
    pub(crate) title: String,
}

/// One task line, with the paths on it and on the indented lines under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Task {
    pub(crate) id: String,
    /// The index of its phase in `TaskList::phases`, or `None` before the
    /// first phase heading.
    pub(crate) phase: Option<usize>,
    /// The `[US<n>]` label's number. A task with none belongs to setup and
    /// polish.
    pub(crate) story: Option<u32>,
    /// Each path once, in the order the task names it. A path that holds
    /// `*` is a glob, kept as written.
    pub(crate) paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TaskList {
    pub(crate) phases: Vec<Phase>,
    pub(crate) tasks: Vec<Task>,
}

/// Reads `text`. Lines that are neither a phase heading, a task line nor a
/// line that belongs to a task are skipped.
pub(crate) fn parse(text: &str) -> TaskList {
    let mut list = TaskList::default();
    // True while the lines that follow still belong to the last task.
    let mut open = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(phase) = phase_heading(line) {
            list.phases.push(phase);
            open = false;
        } else if let Some((id, story, rest)) = task_line(trimmed) {
            let phase = list.phases.len().checked_sub(1);
            let mut task = Task {
                id,
                phase,
                story,
                paths: Vec::new(),
            };
            add_paths(&mut task.paths, rest);
            list.tasks.push(task);
            open = true;
        } else if trimmed.is_empty() || trimmed.starts_with('#') {
            open = false;
        } else if open && trimmed.len() < line.len() {
            if let Some(task) = list.tasks.last_mut() {
                add_paths(&mut task.paths, trimmed);
            }
        } else {
            open = false;
        }
    }
    list
}

/// `## Phase <n>: <title>`.
fn phase_heading(line: &str) -> Option<Phase> {
    let rest = line.strip_prefix("## Phase ")?;
    let (number, title) = rest.split_once(':')?;
    let number = number.trim();
    if number.is_empty() || number.contains(char::is_whitespace) {
        return None;
    }
    Some(Phase {
        number: number.to_owned(),
        title: title.trim().to_owned(),
    })
}

/// `- [ ] T<digits> [P] [US<n>] <text>`, with `[x]` or `[X]` for a done
/// task: the id, the story and the text after the labels.
fn task_line(line: &str) -> Option<(String, Option<u32>, &str)> {
    let rest = ["- [ ] ", "- [x] ", "- [X] "]
        .iter()
        .find_map(|box_| line.strip_prefix(box_))?;
    let (id, mut rest) = rest.split_once(' ').unwrap_or((rest, ""));
    let digits = id.strip_prefix('T')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut story = None;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("[P]") {
            rest = after;
        } else if let Some((n, after)) = rest
            .strip_prefix("[US")
            .and_then(|r| r.split_once(']'))
            .and_then(|(n, after)| n.parse::<u32>().ok().map(|n| (n, after)))
        {
            story = Some(n);
            rest = after;
        } else {
            break;
        }
    }
    Some((id.to_owned(), story, rest))
}

/// One word of a line, and whether it sat inside backticks.
struct Word<'a> {
    text: &'a str,
    code: bool,
}

/// The words of `text`, split at white space and at backticks.
fn words(text: &str) -> Vec<Word<'_>> {
    let mut out = Vec::new();
    let mut code = false;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '`' || c.is_whitespace() {
            if start < i {
                out.push(Word {
                    text: &text[start..i],
                    code,
                });
            }
            if c == '`' {
                code = !code;
            }
            start = i + c.len_utf8();
        }
    }
    if start < text.len() {
        out.push(Word {
            text: &text[start..],
            code,
        });
    }
    out
}

fn add_paths(paths: &mut Vec<String>, text: &str) {
    for word in words(text) {
        for path in paths_of(&word) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
}

/// The paths a word may name, after the punctuation around it is taken off
/// and a brace set such as `src/{a,b}.rs` is expanded. Which of them count
/// depends on the tracked files (`overlap::expand`, FR-013).
fn paths_of(word: &Word<'_>) -> Vec<String> {
    let text = word
        .text
        .trim_start_matches(['(', '[', '"', '\''])
        .trim_end_matches(['.', ',', ';', ':', ')', ']', '"', '\'', '!', '?']);
    // A markdown link's `](...)` is link syntax, not a path the task names.
    if text.contains("](") {
        return Vec::new();
    }
    braces(text)
        .iter()
        .filter_map(|t| path_of(t, word.code))
        .collect()
}

/// `text` with its first brace set of two or more choices expanded, each
/// choice again: `src/{a,b}.rs` gives `src/a.rs` and `src/b.rs`. A brace
/// with no comma, such as a placeholder `{role}`, is left as it is.
fn braces(text: &str) -> Vec<String> {
    let set = text.find('{').and_then(|open| {
        let close = open + text.get(open..)?.find('}')?;
        let inside = text.get(open + 1..close)?;
        inside.contains(',').then_some((open, close, inside))
    });
    let Some((open, close, inside)) = set else {
        return vec![text.to_owned()];
    };
    let (head, tail) = (
        text.get(..open).unwrap_or(""),
        text.get(close + 1..).unwrap_or(""),
    );
    inside
        .split(',')
        .flat_map(|choice| braces(&format!("{head}{}{tail}", choice.trim())))
        .collect()
}

/// The path one word may name: it holds a `/` or ends in `.<ext>`, after a
/// `:N` or `:N-M` line suffix and a `./` are taken off. A URL, a
/// placeholder such as `<role>.md`, an absolute path or route such as
/// `/agui/deck_editor`, and a word outside backticks whose extension is one
/// letter (`e.g.`) name none.
fn path_of(text: &str, code: bool) -> Option<String> {
    if text.contains("://") || text.contains(['<', '>', '{', '}', '|', '=']) {
        return None;
    }
    let text = strip_line_suffix(text);
    let text = text.strip_prefix("./").unwrap_or(text);
    if text.is_empty()
        || text.starts_with(['-', '/'])
        || text.chars().all(|c| matches!(c, '/' | '.'))
    {
        return None;
    }
    let named = text.contains('/') || has_extension(text, if code { 1 } else { 2 });
    named.then(|| text.to_owned())
}

/// `send.rs:297-321` and `send.rs:12` as `send.rs`.
fn strip_line_suffix(text: &str) -> &str {
    let Some((path, suffix)) = text.rsplit_once(':') else {
        return text;
    };
    let mut parts = suffix.splitn(2, '-');
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let first = parts.next().is_some_and(digits);
    let second = parts.next().is_none_or(digits);
    if first && second { path } else { text }
}

/// True when `text` ends in `.<ext>`: a stem, then an extension of at least
/// `min` letters and digits with at least one letter.
pub(crate) fn has_extension(text: &str, min: usize) -> bool {
    let name = text.rsplit('/').next().unwrap_or(text);
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    let stem_ok = !stem.is_empty() || name.starts_with('.');
    stem_ok
        && ext.len() >= min
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        && ext.bytes().any(|b| b.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_of(text: &str, id: &str) -> Vec<String> {
        parse(text)
            .tasks
            .into_iter()
            .find(|t| t.id == id)
            .map(|t| t.paths)
            .unwrap_or_default()
    }

    #[test]
    fn a_line_suffix_is_stripped() {
        let text = "## Phase 3: User Story 1 - x (Priority: P1)\n\n\
            - [ ] T011 [US1] In `crates/teamwire-msg/src/send.rs:297-321`, then `send.rs:12` and \
            `crates/teamwire-cli/src/main.rs:13-48, 84-115`.\n";
        assert_eq!(
            paths_of(text, "T011"),
            [
                "crates/teamwire-msg/src/send.rs",
                "send.rs",
                "crates/teamwire-cli/src/main.rs"
            ]
        );
    }

    #[test]
    fn an_indented_line_belongs_to_the_task_above() {
        let text = "## Phase 3: User Story 1 - x (Priority: P1)\n\n\
            - [X] T015 [US1] Call `look` in the watch:\n  \
              - `crates/teamwire-watch/src/plan.rs` and\n    \
                `crates/teamwire-watch/src/status.rs:137-158`.\n\n  \
              - `docs/after-blank.md`\n\
            - [ ] T016 [US1] Write `ui/src/shared/state.ts`.\n  \
              - `docs/under-t016.md`\n\
            Not indented: `docs/loose.md`\n  \
              - `docs/after-loose.md`\n";
        let list = parse(text);
        assert_eq!(list.tasks.len(), 2);
        assert_eq!(
            paths_of(text, "T015"),
            [
                "crates/teamwire-watch/src/plan.rs",
                "crates/teamwire-watch/src/status.rs"
            ]
        );
        assert_eq!(
            paths_of(text, "T016"),
            ["ui/src/shared/state.ts", "docs/under-t016.md"]
        );
        assert_eq!(list.tasks.first().map(|t| t.story), Some(Some(1)));
    }

    #[test]
    fn a_star_token_is_a_glob() {
        let text = "## Phase 4: User Story 2 - y (Priority: P2)\n\n\
            - [ ] T029 [P] [US2] Keep `crates/x/src/cmd/speckit/*.rs` under 400 lines.\n";
        assert_eq!(paths_of(text, "T029"), ["crates/x/src/cmd/speckit/*.rs"]);
        let list = parse(text);
        assert_eq!(
            list.phases,
            [Phase {
                number: "4".to_owned(),
                title: "User Story 2 - y (Priority: P2)".to_owned()
            }]
        );
        assert_eq!(list.tasks.first().and_then(|t| t.phase), Some(0));
    }

    /// Guard: passes on a stub that returns no tasks. A URL, a placeholder,
    /// an abbreviation, a flag and a lone slash name no path.
    #[test]
    fn a_url_a_placeholder_and_an_abbreviation_are_not_paths() {
        let text = "- [ ] T001 See https://example.com/a/b.html, e.g. `reports/<role>.md`, \
            and run `--repo`, then 1.5 times, and / or `./`.\n";
        assert!(paths_of(text, "T001").is_empty());
    }
}
