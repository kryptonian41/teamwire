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

/// "Fails without it" alone was read as met by any red run; the steps that
/// prove it, and where the proof goes, are what a member can follow.
#[test]
fn an_implementer_proves_each_new_test_fails_on_its_assertion_without_the_fix() {
    let text = render(MemberKind::Implementer);
    assert!(text.contains("see it fail on its assertion"));
    assert!(text.contains("missing name, an import or a constant does not count"));
    assert!(text.contains("prove each branch the test claims to cover"));
    assert!(text.contains("failing\n  assertion line in that commit's progress line"));
}

/// An integrator's brief records a failure the base already has, which the
/// seat's "stop as blocked" would otherwise forbid.
#[test]
fn an_implementer_whose_brief_records_a_base_failure_carries_on() {
    let text = render(MemberKind::Implementer);
    assert!(text.contains("record a failure the base already has"));
    assert!(text.contains("report file and carry on"));
}

/// Verify checks only the spec's owns, so a file only the brief names is asked for.
#[test]
fn an_implementer_edits_only_its_scope_and_asks_about_a_file_only_the_brief_names() {
    let text = render(MemberKind::Implementer);
    assert!(text.contains("Edit only the files listed under YOUR SCOPE below"));
    assert!(text.contains("list leaves out, ask the lead before you edit it"));
}

#[test]
fn an_implementer_treats_an_inline_suppression_as_a_loosened_check() {
    let text = render(MemberKind::Implementer);
    assert!(text.contains("An inline suppression"));
    assert!(text.contains("list each one in your report"));
}
