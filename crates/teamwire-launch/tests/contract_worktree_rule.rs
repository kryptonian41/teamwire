//! Every kind of member is told not to administer git worktrees.

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
