use std::path::Path;

use super::*;

fn render(kind: MemberKind, check: Option<&str>, owns: &[String]) -> String {
    render_contract(&ContractParts {
        team: &TeamName::parse("demo").unwrap(),
        role: &Role::parse("api").unwrap(),
        kind,
        report_file: Path::new("/h/teams/demo/reports/api.md"),
        outbox_dir: Path::new("/h/teams/demo/outbox"),
        teamwire_bin: Path::new("/opt/x/teamwire"),
        check,
        owns,
    })
}

/// Every rule of the speclife report's table 1.2, as a phrase the text must
/// hold. The last field is true for a rule every kind of member carries.
const RULES: &[(&str, &str, bool)] = &[
    (
        "report file is outside the repo; no-write instructions mean the repository",
        "means the repository, never these two files",
        true,
    ),
    (
        "never write to outbox",
        "NEVER write anything into the team's outbox/ directory",
        true,
    ),
    (
        "progress line after each commit",
        "- <short sha> <what the commit did> | left: <what remains>",
        false,
    ),
    (
        "Decisions I made list, none allowed",
        "'Decisions I made: none'",
        true,
    ),
    (
        "two rounds of fixing per failing check, then failed",
        "Two rounds of fixing per failing check, then stop",
        false,
    ),
    (
        "never weaken a check",
        "Never make a check pass by weakening it",
        false,
    ),
    (
        "no installs, no full suite, no repo-wide type gate",
        "Do not run package\n  installs",
        true,
    ),
    (
        "a command that cannot run at all: record it and set blocked",
        "set status 'blocked', and finish",
        true,
    ),
    (
        "only claim a check passed if you ran it",
        "Only claim a check passed if you actually ran it",
        true,
    ),
];

#[test]
fn every_recorded_rule_is_in_the_implementer_contract() {
    let text = render(
        MemberKind::Implementer,
        Some("cargo test"),
        &["src/".to_owned()],
    );
    for (rule, phrase, _) in RULES {
        assert!(text.contains(phrase), "missing rule: {rule}");
    }
}

#[test]
fn shared_rules_are_in_every_kind() {
    for kind in MemberKind::ALL {
        let text = render(*kind, None, &[]);
        for (rule, phrase, _) in RULES.iter().filter(|r| r.2) {
            assert!(text.contains(phrase), "{kind}: missing rule: {rule}");
        }
    }
}

#[test]
fn reviewer_and_closer_send_and_receive_no_messages() {
    for kind in [MemberKind::Reviewer, MemberKind::Closer] {
        let text = render(kind, None, &[]);
        assert!(text.contains("You send no messages, and members do not message you"));
        assert!(!text.contains("teamwire send"));
    }
    let text = render(MemberKind::Implementer, None, &[]);
    assert!(text.contains("/opt/x/teamwire send --to <role or lead>"));
}

#[test]
fn only_the_investigator_gets_the_finding_rule_and_only_the_implementer_the_notice_rule() {
    let inv = render(MemberKind::Investigator, None, &[]);
    let imp = render(MemberKind::Implementer, None, &[]);
    assert!(inv.contains("- finding:") && !inv.contains("- notice:"));
    assert!(imp.contains("- notice:") && !imp.contains("- finding:"));
}

#[test]
fn scope_block_lists_owns_and_check_and_is_absent_without_them() {
    let with = render(
        MemberKind::Implementer,
        Some("cargo test -p demo"),
        &["crates/demo/".to_owned(), "docs/x.md".to_owned()],
    );
    assert!(with.contains("YOUR SCOPE (from the team spec"));
    assert!(with.contains("    crates/demo/\n    docs/x.md\n"));
    assert!(with.contains("    cargo test -p demo\n"));
    assert!(!render(MemberKind::Implementer, None, &[]).contains("YOUR SCOPE (from the team spec"));
}

#[test]
fn names_and_paths_are_filled_and_no_placeholder_is_left() {
    let text = render(MemberKind::Implementer, None, &[]);
    assert!(text.contains("You are 'api', an engineer on team 'demo'"));
    assert!(text.contains("/h/teams/demo/reports/api.md"));
    assert!(text.contains("(/h/teams/demo/outbox)"));
    assert!(!text.contains('{'), "an unfilled placeholder");
}

#[test]
fn text_names_only_teamwire_commands() {
    for kind in MemberKind::ALL {
        let text = render(*kind, Some("true"), &["a/".to_owned()]);
        for banned in ["team-msg", ".py", "python", "~/.claude", "herdr", "done/"] {
            assert!(!text.contains(banned), "{kind}: mentions {banned}");
        }
        assert!(text.contains("/opt/x/teamwire done --status done|blocked|failed"));
    }
}

#[test]
fn contract_names_the_launching_binary() {
    for kind in MemberKind::ALL {
        let text = render(*kind, Some("true"), &["a/".to_owned()]);
        let commands: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| l.contains(" done ") || l.contains(" send --to"))
            .collect();
        assert!(!commands.is_empty(), "{kind}: no command line");
        for line in commands {
            assert!(line.starts_with("/opt/x/teamwire "), "{kind}: {line}");
        }
        assert!(text.contains("`/opt/x/teamwire done` again"));
        assert!(
            !text.contains("{teamwire}"),
            "{kind}: an unfilled placeholder"
        );
        assert!(!text.contains('{'), "{kind}: an unfilled placeholder");
    }
}
