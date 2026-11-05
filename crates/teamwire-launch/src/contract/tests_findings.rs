//! Rules added after the 2026-11-05 review of every member instruction. Each
//! test pins the phrase a member acts on, so a rewording that drops the rule
//! fails here.

use std::path::Path;

use super::*;

fn render(kind: MemberKind) -> String {
    render_contract(&ContractParts {
        team: &TeamName::parse("demo").unwrap(),
        role: &Role::parse("api").unwrap(),
        kind,
        report_file: Path::new("/h/teams/demo/reports/api.md"),
        outbox_dir: Path::new("/h/teams/demo/outbox"),
        teamwire_bin: Path::new("/opt/x/teamwire"),
        check: None,
        owns: &[],
    })
}

/// The lead reads only the top of a report, so every kind is told to put a
/// summary there, both where the report file is named and at the finish.
#[test]
fn every_kind_starts_its_report_with_a_short_summary() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(
            text.contains("Start it with a summary of at most ten lines"),
            "{kind}"
        );
        assert!(
            text.contains("- it starts with a summary of at most ten lines;"),
            "{kind}"
        );
    }
}

/// The agent file's own gate commands would contradict "run only your check".
#[test]
fn an_implementer_runs_only_the_agent_file_commands_its_scope_names() {
    let text = render(MemberKind::Implementer);
    assert!(text.contains("on how code is written; its rules are CI\n  gates"));
    assert!(text.contains("run only the ones YOUR SCOPE or your brief names"));
}
