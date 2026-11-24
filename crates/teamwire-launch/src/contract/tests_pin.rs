//! ADR 0059: what every contract says about a long reprompt's pointer line
//! and a result, and how a closer pins its checks
//! to the commit verify recorded.

use std::path::Path;

use super::*;

/// The contract with every run of whitespace folded to one space, so a
/// phrase is found wherever the text wraps it.
fn render(kind: MemberKind) -> String {
    let text = render_contract(&ContractParts {
        team: &TeamName::parse("demo").unwrap(),
        role: &Role::parse("api").unwrap(),
        kind,
        report_file: Path::new("/h/teams/demo/reports/api.md"),
        outbox_dir: Path::new("/h/teams/demo/outbox"),
        teamwire_bin: Path::new("/opt/x/teamwire"),
        check: None,
        owns: &[],
        quiet: false,
    });
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

const KINDS: [MemberKind; 6] = [
    MemberKind::Implementer,
    MemberKind::Investigator,
    MemberKind::Reviewer,
    MemberKind::Closer,
    MemberKind::PlanReviewer,
    MemberKind::Reconciler,
];

#[test]
fn every_contract_says_a_long_reprompt_is_a_pointer_and_a_reminder_is_not_reading() {
    for kind in KINDS {
        let text = render(kind);
        assert!(
            text.contains("A long reprompt arrives as a pointer"),
            "{kind}"
        );
        assert!(
            text.contains("A reminder after a tool call is not reading the message"),
            "{kind}"
        );
        assert!(
            text.contains("An answer in chat or a finding is not a result"),
            "{kind}"
        );
    }
}

#[test]
fn the_closer_pins_every_check_to_the_commit_verify_recorded() {
    let closer = render(MemberKind::Closer);
    for phrase in [
        "the `head` line `teamwire verify` prints",
        "git checkout --quiet --detach <sha>",
        "/opt/x/teamwire verify demo --role <role> --pinned-in",
        "A branch name is not a commit",
        "never with `|| true`",
        "no `$(...)` or other placeholder",
        "A check that also passes on the base commit proves nothing",
        "no verify record, or an out-of-date one",
        "the joined head",
        "An exit 0 proves the commit, not any file or check",
        "use only the clone you made with `git clone`",
    ] {
        assert!(closer.contains(phrase), "missing: {phrase}");
    }
    // The old advice that let a branch name stand for the commit is gone.
    assert!(!closer.contains("check out the member's branch there"));
}
