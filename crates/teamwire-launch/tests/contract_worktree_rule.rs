//! Rules every kind of member is told: never administer git worktrees, and
//! record a result again after a message wakes it.

use std::path::Path;

use teamwire_core::{MemberKind, Role, TeamName};
use teamwire_launch::{ContractParts, render_contract};

#[test]
fn every_kind_is_told_not_to_administer_worktrees() {
    for kind in MemberKind::ALL {
        let text = render_contract(&ContractParts {
            team: &TeamName::parse("demo").unwrap(),
            role: &Role::parse("api").unwrap(),
            kind: *kind,
            report_file: Path::new("/h/teams/demo/reports/api.md"),
            outbox_dir: Path::new("/h/teams/demo/outbox"),
            teamwire_bin: Path::new("/opt/x/teamwire"),
            check: None,
            owns: &[],
        });
        assert!(
            text.contains(
                "Never run `git worktree add`, `git worktree remove`, `git worktree move`\n  or `git worktree prune`"
            )
                && text.contains("never\n  write into another member's directory")
                && text.contains("report blocked to the\n  lead"),
            "{kind}"
        );
    }
}

/// A message to a finished member starts a new generation (ADR 0043), and the
/// watch reports the member silent until it records a result in it.
#[test]
fn every_kind_is_told_to_record_done_again_after_a_message_wakes_it() {
    for kind in MemberKind::ALL {
        let text = render_contract(&ContractParts {
            team: &TeamName::parse("demo").unwrap(),
            role: &Role::parse("api").unwrap(),
            kind: *kind,
            report_file: Path::new("/h/teams/demo/reports/api.md"),
            outbox_dir: Path::new("/h/teams/demo/outbox"),
            teamwire_bin: Path::new("/opt/x/teamwire"),
            check: None,
            owns: &[],
        });
        assert!(
            text.contains(
                "A teamwire message that reaches you after you recorded your completion\n\
                 and stopped, from the parent, the user or a teammate, starts a new turn\n\
                 that the parent's watch follows, even when the message asks for nothing.\n\
                 Record your completion again before you stop, with what you did or why\n\
                 nothing changed. Without it the watch reports you as silent."
            ),
            "{kind}"
        );
    }
}
