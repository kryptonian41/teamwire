//! What the tasks of a `tasks.md` share (ADR 0044 decision 3): the files
//! each pair of phases and each pair of stories names, the tasks that name
//! no file, and the tasks whose files no other task names.

use std::collections::{BTreeMap, BTreeSet};

use teamwire_verify::Owns;

use super::parse::{TaskList, has_extension};

/// The files a pair of phases, or a pair of stories, both name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shared<K> {
    pub(crate) first: K,
    pub(crate) second: K,
    pub(crate) paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Overlap {
    /// By index into `TaskList::phases`.
    pub(crate) phases: Vec<Shared<usize>>,
    /// By story number.
    pub(crate) stories: Vec<Shared<u32>>,
    /// The ids of the tasks that name no file.
    pub(crate) no_path: Vec<String>,
    /// The tasks whose files no other task names, with those files.
    pub(crate) independent: Vec<(String, Vec<String>)>,
}

/// The words of `paths` that are repository paths (FR-013), with each glob
/// (a path that holds `*`) replaced by the tracked files it matches, as
/// verify's `owns` matches them. A glob that matches no tracked file stays
/// as itself, like a path that does not exist yet.
///
/// A word with a `/` is a path when it names a tracked file, a tracked
/// folder (kept with a trailing `/`), or ends in an extension, as a file a
/// task creates does. A word with no `/` is a path only when it names a
/// tracked file at the repository's root, such as `README.md`: an event
/// name such as `knit.progress` or a bare `lib.rs` is not.
pub(crate) fn expand(paths: &[String], tracked: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |p: &str| {
        if !out.iter().any(|o| o == p) {
            out.push(p.to_owned());
        }
    };
    for path in paths {
        if !path.contains('*') {
            if let Some(p) = repo_path(path, tracked) {
                add(&p);
            }
            continue;
        }
        let owns = Owns::new(std::slice::from_ref(path));
        let matched: Vec<&String> = tracked.iter().filter(|t| owns.matches(t)).collect();
        if matched.is_empty() {
            add(path);
        }
        for m in matched {
            add(m);
        }
    }
    out
}

/// `word` as a repository path, or `None` when it names none (see `expand`).
fn repo_path(word: &str, tracked: &[String]) -> Option<String> {
    if !word.contains('/') {
        return tracked.iter().any(|t| t == word).then(|| word.to_owned());
    }
    let bare = word.trim_end_matches('/');
    if !word.ends_with('/') && tracked.iter().any(|t| t == bare) {
        return Some(bare.to_owned());
    }
    let folder = format!("{bare}/");
    if tracked.iter().any(|t| t.starts_with(&folder)) {
        return Some(folder);
    }
    (!word.ends_with('/') && has_extension(word, 1)).then(|| word.to_owned())
}

/// True when `a` and `b` name the same file, or one is a folder (with a
/// trailing `/`) that holds the other.
fn meets(a: &str, b: &str) -> bool {
    a == b || (a.ends_with('/') && b.starts_with(a)) || (b.ends_with('/') && a.starts_with(b))
}

/// The overlap of `list`, where `paths[i]` is the expanded paths of task `i`.
pub(crate) fn overlap(list: &TaskList, paths: &[Vec<String>]) -> Overlap {
    let mut by_phase: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
    let mut by_story: BTreeMap<u32, BTreeSet<&str>> = BTreeMap::new();
    let mut out = Overlap::default();
    for (task, paths) in list.tasks.iter().zip(paths) {
        if paths.is_empty() {
            out.no_path.push(task.id.clone());
        }
        for p in paths {
            if let Some(phase) = task.phase {
                by_phase.entry(phase).or_default().insert(p);
            }
            if let Some(story) = task.story {
                by_story.entry(story).or_default().insert(p);
            }
        }
    }
    out.phases = pairs(&by_phase);
    out.stories = pairs(&by_story);
    for (i, (task, mine)) in list.tasks.iter().zip(paths).enumerate() {
        let alone = paths
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .all(|(_, theirs)| !mine.iter().any(|p| theirs.iter().any(|q| meets(p, q))));
        if !mine.is_empty() && alone {
            out.independent.push((task.id.clone(), mine.clone()));
        }
    }
    out
}

/// Every pair of keys whose path sets meet, in key order, with the paths
/// they share. A folder in one set shares each file of the other set that
/// sits under it, and that file is listed.
fn pairs<K: Copy + Ord>(sets: &BTreeMap<K, BTreeSet<&str>>) -> Vec<Shared<K>> {
    let mut out = Vec::new();
    for (i, (first, a)) in sets.iter().enumerate() {
        for (second, b) in sets.iter().skip(i + 1) {
            let within = |p: &str, other: &BTreeSet<&str>| {
                other
                    .iter()
                    .any(|q| q.ends_with('/') && p.starts_with(q) && p != *q)
            };
            let mut both: BTreeSet<&str> = a.intersection(b).copied().collect();
            both.extend(a.iter().copied().filter(|p| within(p, b)));
            both.extend(b.iter().copied().filter(|p| within(p, a)));
            let shared: Vec<String> = both.into_iter().map(str::to_owned).collect();
            if !shared.is_empty() {
                out.push(Shared {
                    first: *first,
                    second: *second,
                    paths: shared,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::speckit::parse::parse;

    const TASKS: &str = "## Phase 1: Setup\n\n\
        - [ ] T001 Read the plan.\n\n\
        ## Phase 3: User Story 1 - a (Priority: P1)\n\n\
        - [ ] T003 [US1] Change `src/a.rs` and `src/b.rs`.\n\n\
        ## Phase 4: User Story 2 - b (Priority: P2)\n\n\
        - [ ] T004 [US2] Change `src/a.rs:3`.\n\
        - [ ] T005 [US2] Write `src/c.rs`.\n";

    fn run(text: &str) -> Overlap {
        let list = parse(text);
        let paths: Vec<Vec<String>> = list.tasks.iter().map(|t| t.paths.clone()).collect();
        overlap(&list, &paths)
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn phases_and_stories_that_name_one_file_share_it() {
        let o = run(TASKS);
        assert_eq!(
            o.phases,
            [Shared {
                first: 1,
                second: 2,
                paths: strings(&["src/a.rs"])
            }]
        );
        assert_eq!(
            o.stories,
            [Shared {
                first: 1,
                second: 2,
                paths: strings(&["src/a.rs"])
            }]
        );
    }

    #[test]
    fn tasks_with_no_file_and_tasks_on_their_own_files_are_listed() {
        let o = run(TASKS);
        assert_eq!(o.no_path, ["T001"]);
        assert_eq!(o.independent, [("T005".to_owned(), strings(&["src/c.rs"]))]);
    }

    #[test]
    fn a_star_token_expands_against_tracked_files() {
        let tracked = strings(&[
            "crates/x/src/cmd/speckit/one.rs",
            "crates/x/src/cmd/speckit/two.rs",
            "crates/x/src/cmd/speckit/deep/three.rs",
            "crates/x/src/cmd/other.rs",
        ]);
        let paths = strings(&["crates/x/src/cmd/speckit/*.rs", "src/new/*.rs", "src/a.rs"]);
        assert_eq!(
            expand(&paths, &tracked),
            strings(&[
                "crates/x/src/cmd/speckit/one.rs",
                "crates/x/src/cmd/speckit/two.rs",
                "src/new/*.rs",
                "src/a.rs"
            ])
        );
    }
}
