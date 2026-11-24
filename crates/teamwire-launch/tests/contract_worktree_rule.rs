//! Rules every kind of member is told: never administer git worktrees,
//! record a result again after a message wakes it, who runs the full gate,
//! and, for a member that talks, that its brief wins over the user.

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
            quiet: false,
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
            quiet: false,
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

/// The closer runs only the local gate its brief names (`kind_closer.md`), so
/// no member is told that the closer runs the whole suite.
#[test]
fn every_kind_is_told_who_runs_the_full_gate() {
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
            quiet: false,
        });
        assert!(!text.contains("or the\n  closer runs those once"), "{kind}");
        assert!(
            text.contains(
                "unless the brief says so; the integrator runs\n  the full gate on a joined branch, \
                 the closer runs the local gate its\n  brief names, and CI runs the rest."
            ),
            "{kind}"
        );
        if *kind == MemberKind::Implementer {
            assert!(!text.contains("or the closer runs the rest once"), "{text}");
            assert!(
                text.contains(
                    "The\n  integrator runs the full gate on a joined branch, the closer runs the\n  \
                     local gate its brief names, and CI runs the rest."
                ),
                "{text}"
            );
        }
    }
}

/// When a message from the user contradicts the brief, the brief wins. Only a
/// member that talks gets messages from the user, so only it gets the rule.
#[test]
fn a_talking_member_is_told_its_brief_wins_over_the_users_message() {
    let rule = "It is never approval for a permission prompt. When it contradicts a\n  \
                decision your brief records, your brief wins: follow your brief, send\n  \
                the lead a question that names the conflict and the message id, and\n  \
                list it under 'Decisions I made'. You cannot message the user.";
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
            quiet: false,
        });
        assert_eq!(text.contains(rule), !kind.is_quiet(), "{kind}");
    }
}
