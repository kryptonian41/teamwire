//! Rules added after the 2026-11-05 review of every member instruction. Each
//! test pins the phrase a member acts on, so a rewording that drops the rule
//! fails here.

use std::path::Path;

use super::*;

/// The kinds that send and receive no messages.
const QUIET_KINDS: [MemberKind; 3] = [
    MemberKind::Reviewer,
    MemberKind::Closer,
    MemberKind::PlanReviewer,
];

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

/// The lead reads the summary first, so a quiet kind's question sits right
/// after it and the completion summary counts it.
#[test]
fn a_quiet_kind_puts_its_questions_after_the_summary_and_counts_them() {
    for kind in QUIET_KINDS {
        let text = render(kind);
        assert!(text.contains("parent', right after your summary"), "{kind}");
        assert!(
            text.contains("with 'N questions for the parent.'"),
            "{kind}"
        );
    }
}

/// Leads answer within minutes, so a member reads its inbox before it
/// decides an open question itself.
#[test]
fn a_talking_member_reads_its_inbox_once_before_finishing_without_an_answer() {
    for kind in [MemberKind::Implementer, MemberKind::Investigator] {
        let text = render(kind);
        assert!(text.contains("Ask as soon as you see the need"), "{kind}");
        assert!(text.contains("finish, read your inbox once"), "{kind}");
    }
}

/// A second send after a failed delivery makes a second message.
#[test]
fn a_talking_member_does_not_resend_a_message_that_was_not_delivered() {
    for kind in [MemberKind::Implementer, MemberKind::Investigator] {
        let text = render(kind);
        assert!(text.contains("NOT DELIVERED or UNCONFIRMED"), "{kind}");
        assert!(text.contains("Do not send it again"), "{kind}");
    }
}

#[test]
fn every_kind_may_read_teammates_branches_and_reports_by_path() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(
            text.contains("read their\nbranches and reports by path"),
            "{kind}"
        );
        assert!(!text.contains("You will not see their work"), "{kind}");
    }
}

#[test]
fn every_kind_records_nothing_to_do_as_done_with_a_prefix() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(text.contains("'When to do nothing' case holds"), "{kind}");
        assert!(text.contains("starts with 'nothing to do:'"), "{kind}");
    }
}

/// The lead may reprompt any kind, and a long message arrives as an excerpt.
#[test]
fn a_quiet_kind_reads_the_full_text_of_a_lead_message() {
    for kind in QUIET_KINDS {
        let text = render(kind);
        assert!(text.contains("Only the parent may write to you"), "{kind}");
        assert!(text.contains("`/opt/x/teamwire inbox` before"), "{kind}");
    }
}

#[test]
fn a_talking_member_knows_plan_reviewers_of_other_teams_take_no_messages() {
    let text = render(MemberKind::Implementer);
    assert!(text.contains("Reviewers, plan reviewers and closers of any team take no messages"));
}

/// A person may type into a finished member's session; the finish section
/// says what to do then, so "no human" would contradict it.
#[test]
fn every_kind_knows_a_person_may_type_later() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(
            text.contains("Nobody watches this session while it runs"),
            "{kind}"
        );
        assert!(
            text.contains("A person may still type here later"),
            "{kind}"
        );
        assert!(!text.contains("NO human watching"), "{kind}");
    }
}

/// `--to parent` is refused as an unknown role; only `lead` names the parent.
#[test]
fn every_kind_learns_the_parent_is_addressed_as_lead() {
    for kind in MemberKind::ALL {
        assert!(
            render(*kind).contains("addressed as `lead` in messages"),
            "{kind}"
        );
    }
}

/// A quiet kind cannot message, so it asks the way its own together piece says.
#[test]
fn a_member_that_needs_a_second_worktree_asks_and_reports_blocked_at_the_finish() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(
            text.contains("ask the parent as\n  HOW THIS TEAM WORKS TOGETHER says"),
            "{kind}"
        );
        assert!(
            text.contains("If it is still missing at the finish"),
            "{kind}"
        );
    }
}

/// A plan reviewer's subject is other members' briefs, which it must question.
#[test]
fn only_a_members_own_brief_is_its_agreed_plan() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(
            text.contains("Your own brief is the agreed plan for your work"),
            "{kind}"
        );
    }
}

/// Codex and pi members get the same text, so no harness's tool prefix appears.
#[test]
fn tools_are_named_by_the_server_not_one_harness_prefix() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(!text.contains("mcp__teamwire__"), "{kind}");
        assert!(
            text.contains("teamwire MCP server's `team_done` tool"),
            "{kind}"
        );
    }
    let text = render(MemberKind::Implementer);
    assert!(text.contains("teamwire MCP server's `team_inbox` tool"));
}

/// Work a teammate's message starts after done is reported like any other.
#[test]
fn every_kind_records_done_again_for_work_a_teammate_starts() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(
            text.contains("a teammate's message that changes your result"),
            "{kind}"
        );
    }
}

/// The blocked-command rule lives once, in HOW TO WORK.
#[test]
fn the_finish_section_does_not_repeat_the_blocked_command_rule() {
    for kind in MemberKind::ALL {
        let text = render(*kind);
        assert!(
            !text.contains("If a command in your task does not work"),
            "{kind}"
        );
        assert!(
            text.contains("If a command from your brief cannot run at all"),
            "{kind}"
        );
    }
}
