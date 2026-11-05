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

/// The closer runs before any graded finding has a fix, so the revert check
/// names what exists then: claimed tests and fixes of a reviewer's findings.
#[test]
fn a_closer_reverts_claimed_tests_and_fixes_of_reviewer_findings() {
    let text = render(MemberKind::Closer);
    assert!(text.contains("reviewer reported and an owner fixed"));
    assert!(!text.contains("graded wrong conclusion"));
}

/// A repository hook may refuse `git reset --hard`; the closer gets one
/// sanctioned way to undo a revert instead of inventing one.
#[test]
fn a_closer_undoes_a_revert_by_named_files_when_a_hook_refuses_reset() {
    let text = render(MemberKind::Closer);
    assert!(text.contains("do not work around the hook"));
    assert!(text.contains("`git restore --source=HEAD --staged --worktree -- <files>`"));
    assert!(text.contains("`git status --porcelain`"));
    assert!(text.contains("add `-e <path>` to `git clean`"));
}

/// The setup command is stored nowhere a member can read, and `sh -c` is forbidden.
#[test]
fn a_closer_takes_each_setup_from_its_brief_and_runs_it_without_a_wrapper() {
    let text = render(MemberKind::Closer);
    assert!(text.contains("Your brief names each member's setup command"));
    assert!(text.contains("put the clone's path where the command reads `$1`"));
    assert!(!text.contains("as its first argument"));
}

#[test]
fn a_reviewer_runs_tests_in_its_own_clone_and_finds_the_implementer_brief() {
    let text = render(MemberKind::Reviewer);
    assert!(text.contains("cache/closer/$TEAMWIRE_TEAM/review-$TEAMWIRE_ROLE"));
    assert!(text.contains("even when your brief offers one"));
    assert!(text.contains("briefs/<implementer's role>.md"));
}

#[test]
fn a_plan_reviewer_greps_the_whole_repository_for_unowned_files() {
    let text = render(MemberKind::PlanReviewer);
    assert!(
        text.contains("Grep the\n    whole repository for each name or value the brief changes")
    );
}

/// Every kind gets the background-job rule once, from the shared piece.
#[test]
fn every_kind_gets_the_background_job_rule_once() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        let rule = "Do not end a turn while a background job you still need is running";
        assert_eq!(text.matches(rule).count(), 1, "{kind}");
    }
}

/// A notice needs a commit and an answer needs a question, so a member that
/// commits nothing is told which type carries any other fact or reply.
#[test]
fn a_talking_member_sends_other_facts_and_replies_as_findings() {
    for kind in [MemberKind::Implementer, MemberKind::Investigator] {
        let text = render(kind);
        assert!(text.contains("- notice: only after you COMMIT"), "{kind}");
        assert!(text.contains("Send any other fact as a finding"), "{kind}");
        assert!(
            text.contains("To reply to a finding or a notice, send a finding"),
            "{kind}"
        );
    }
}
