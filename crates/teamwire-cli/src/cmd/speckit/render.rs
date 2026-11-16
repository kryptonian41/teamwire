//! The text `teamwire speckit draft` prints (`docs/api/cli.md`): the shared
//! files, the tasks that can run apart, the unread workflows, the names
//! that are no repository path and the DRAFT `owns` lists.

use super::mentions::{Found, Unread};
use super::overlap::Overlap;
use super::parse::TaskList;

/// The label before the lists. The lead checks each list before a launch.
pub(crate) const DRAFT_LABEL: &str =
    "DRAFT owns lists. Check each one before you launch: these are drafts, not final lists.";

/// The head of the words that are no repository path. They go in no
/// draft; the lead checks each for a new file in a new folder.
pub(crate) const NOT_PATHS: &str = "Names that are not repository paths";

/// The head of the bare task words read as root files (ADR 0049 rule C).
/// They stay in the drafts; the lead checks each, since a bare name may
/// mean a file in a folder the task names earlier in the line.
pub(crate) const BARE_ROOTS: &str = "Bare names read as root files";

/// One DRAFT `owns` list: its name (`US1`, or `Setup and polish`) and its
/// files with their reasons.
pub(crate) struct Draft {
    pub(crate) name: String,
    pub(crate) files: Vec<Found>,
}

pub(crate) struct Report<'a> {
    /// The `tasks.md` path as the lead named it.
    pub(crate) tasks_path: &'a str,
    pub(crate) list: &'a TaskList,
    pub(crate) overlap: &'a Overlap,
    pub(crate) unread: &'a [Unread],
    /// Each task id with its words that are no repository path (ADR 0047).
    pub(crate) not_paths: &'a [(String, Vec<String>)],
    /// Each task id with its bare words read as root files (ADR 0049).
    pub(crate) bare_roots: &'a [(String, Vec<String>)],
    pub(crate) drafts: &'a [Draft],
}

/// The whole output, ending in a newline.
pub(crate) fn render(r: &Report<'_>) -> String {
    let mut out: Vec<String> = Vec::new();
    let stories = r
        .list
        .tasks
        .iter()
        .filter_map(|t| t.story)
        .collect::<std::collections::BTreeSet<u32>>()
        .len();
    out.push(format!(
        "spec-kit tasks: {} ({} tasks, {} phases, {} stories)",
        r.tasks_path,
        r.list.tasks.len(),
        r.list.phases.len(),
        stories
    ));
    out.push(String::new());

    let phase_name = |i: usize| {
        r.list
            .phases
            .get(i)
            .map_or_else(|| "Phase ?".to_owned(), |p| format!("Phase {}", p.number))
    };
    if r.overlap.phases.is_empty() {
        out.push("Shared files between phases: none".to_owned());
    } else {
        out.push("Shared files between phases:".to_owned());
        for s in &r.overlap.phases {
            out.push(format!(
                "  {} and {}: {}",
                phase_name(s.first),
                phase_name(s.second),
                s.paths.join(", ")
            ));
        }
    }
    let stories_line: Vec<String> = r
        .overlap
        .stories
        .iter()
        .map(|s| format!("US{} and US{} ({})", s.first, s.second, s.paths.join(", ")))
        .collect();
    out.push(format!(
        "Stories that share files: {}",
        or_none(&stories_line.join("; "))
    ));
    out.push(String::new());

    out.push(format!(
        "Tasks that change no file: {}",
        or_none(&r.overlap.no_path.join(", "))
    ));
    let alone: Vec<String> = r
        .overlap
        .independent
        .iter()
        .map(|(id, paths)| format!("{id} ({})", paths.join(", ")))
        .collect();
    out.push(format!(
        "Tasks whose files no other task names: {}",
        or_none(&alone.join(", "))
    ));
    out.push(String::new());

    let unread: Vec<String> = r
        .unread
        .iter()
        .map(|u| format!("{} ({})", u.path, u.why))
        .collect();
    out.push(format!("Unread workflows: {}", or_none(&unread.join(", "))));
    out.push(String::new());

    for (head, by_task) in [(NOT_PATHS, r.not_paths), (BARE_ROOTS, r.bare_roots)] {
        if by_task.is_empty() {
            out.push(format!("{head}: none"));
        } else {
            out.push(format!("{head}:"));
            for (id, names) in by_task {
                out.push(format!("  {id}: {}", names.join(", ")));
            }
        }
        out.push(String::new());
    }

    out.push(DRAFT_LABEL.to_owned());
    for d in r.drafts {
        out.push(format!("{}:", d.name));
        let width = d.files.iter().map(|f| f.path.len()).max().unwrap_or(0);
        for f in &d.files {
            out.push(format!("  {:width$}  ({})", f.path, f.reason));
        }
    }
    out.join("\n") + "\n"
}

fn or_none(text: &str) -> &str {
    if text.is_empty() { "none" } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::speckit::overlap::Shared;
    use crate::cmd::speckit::parse::parse;

    #[test]
    fn empty_sections_say_none_and_the_reasons_line_up() {
        let list = parse("## Phase 3: User Story 1 - a\n\n- [ ] T001 [US1] Change `src/a.rs`.\n");
        let overlap = Overlap::default();
        let drafts = [Draft {
            name: "US1".to_owned(),
            files: vec![
                Found {
                    path: "src/a.rs".to_owned(),
                    reason: "task path".to_owned(),
                },
                Found {
                    path: ".github/workflows/replay.yml".to_owned(),
                    reason: "CI paths glob src/**".to_owned(),
                },
            ],
        }];
        let text = render(&Report {
            tasks_path: "specs/001-x/tasks.md",
            list: &list,
            overlap: &overlap,
            unread: &[],
            not_paths: &[],
            bare_roots: &[],
            drafts: &drafts,
        });
        assert_eq!(
            text,
            "spec-kit tasks: specs/001-x/tasks.md (1 tasks, 1 phases, 1 stories)\n\
             \n\
             Shared files between phases: none\n\
             Stories that share files: none\n\
             \n\
             Tasks that change no file: none\n\
             Tasks whose files no other task names: none\n\
             \n\
             Unread workflows: none\n\
             \n\
             Names that are not repository paths: none\n\
             \n\
             Bare names read as root files: none\n\
             \n\
             DRAFT owns lists. Check each one before you launch: these are drafts, not final lists.\n\
             US1:\n  \
               src/a.rs                      (task path)\n  \
               .github/workflows/replay.yml  (CI paths glob src/**)\n"
        );
    }

    #[test]
    fn story_pairs_are_joined_with_semicolons() {
        let list = parse("- [ ] T001 [US1] x\n");
        let overlap = Overlap {
            stories: vec![
                Shared {
                    first: 1,
                    second: 2,
                    paths: vec!["a.rs".to_owned(), "b.rs".to_owned()],
                },
                Shared {
                    first: 1,
                    second: 3,
                    paths: vec!["c.rs".to_owned()],
                },
            ],
            ..Overlap::default()
        };
        let text = render(&Report {
            tasks_path: "t.md",
            list: &list,
            overlap: &overlap,
            unread: &[],
            not_paths: &[],
            bare_roots: &[],
            drafts: &[],
        });
        assert!(
            text.contains(
                "Stories that share files: US1 and US2 (a.rs, b.rs); US1 and US3 (c.rs)\n"
            ),
            "{text}"
        );
    }

    /// ADR 0047 rule 4: each task's names that are no repository path, by
    /// task id.
    #[test]
    fn names_that_are_no_path_are_listed_by_task() {
        let list = parse("- [ ] T010 [US1] x\n- [ ] T011 [US1] y\n");
        let not_paths = [
            (
                "T010".to_owned(),
                vec!["crates/b/**".to_owned(), "crates/c/**".to_owned()],
            ),
            (
                "T011".to_owned(),
                vec!["specs/NNN-name/tasks.md".to_owned()],
            ),
        ];
        let text = render(&Report {
            tasks_path: "t.md",
            list: &list,
            overlap: &Overlap::default(),
            unread: &[],
            not_paths: &not_paths,
            bare_roots: &[],
            drafts: &[],
        });
        assert!(
            text.contains(
                "\nNames that are not repository paths:\n  \
                 T010: crates/b/**, crates/c/**\n  \
                 T011: specs/NNN-name/tasks.md\n\n"
            ),
            "{text}"
        );
    }

    /// ADR 0049 rule C: each task's bare words read as root files, by task
    /// id, right after the names that are no repository path.
    #[test]
    fn bare_names_read_as_root_files_are_listed_by_task() {
        let list = parse("- [ ] T004 x\n- [ ] T065 y\n");
        let bare_roots = [
            (
                "T004".to_owned(),
                vec!["AGENTS.md".to_owned(), "CLAUDE.md".to_owned()],
            ),
            (
                "T065".to_owned(),
                vec!["sonar-project.properties".to_owned()],
            ),
        ];
        let text = render(&Report {
            tasks_path: "t.md",
            list: &list,
            overlap: &Overlap::default(),
            unread: &[],
            not_paths: &[],
            bare_roots: &bare_roots,
            drafts: &[],
        });
        assert!(
            text.contains(
                "Names that are not repository paths: none\n\n\
                 Bare names read as root files:\n  \
                 T004: AGENTS.md, CLAUDE.md\n  \
                 T065: sonar-project.properties\n\n"
            ),
            "{text}"
        );
    }
}
