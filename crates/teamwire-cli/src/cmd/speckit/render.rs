//! The text `teamwire speckit draft` prints (`docs/api/cli.md`): the shared
//! files, the tasks that can run apart, the unread workflows and the DRAFT
//! `owns` lists.

use super::mentions::{Found, Unread};
use super::overlap::Overlap;
use super::parse::TaskList;

/// The label before the lists. The lead checks each list before a launch.
pub(crate) const DRAFT_LABEL: &str =
    "DRAFT owns lists. Check each one before you launch: these are drafts, not final lists.";

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
            drafts: &[],
        });
        assert!(
            text.contains(
                "Stories that share files: US1 and US2 (a.rs, b.rs); US1 and US3 (c.rs)\n"
            ),
            "{text}"
        );
    }
}
