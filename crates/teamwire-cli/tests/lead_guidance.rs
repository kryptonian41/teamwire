//! The lead skill states what counts as a sent reprompt and a finished
//! generation (ADR 0059). A lead that read a refused send as a receipt, then
//! closed the team with a bare `teamwire close`, archived a blocked
//! generation as a success. The skill is the lead's only copy of these
//! rules, for a Claude Code lead and a pi lead alike.

use std::path::Path;

fn example(path: &str) -> String {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(path),
    )
    .unwrap();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The part of `text` from the heading `from` to the next heading at the
/// same depth.
fn section(text: &str, from: &str) -> String {
    let start = text
        .find(from)
        .unwrap_or_else(|| unreachable!("no section {from}"));
    let depth = from.split(' ').next().unwrap_or_default();
    let rest = &text[start + from.len()..];
    let end = rest.find(&format!(" {depth} ")).unwrap_or(rest.len());
    rest[..end].to_owned()
}

#[test]
fn the_messages_section_says_what_a_receipt_is_and_what_a_long_reprompt_looks_like() {
    let messages = section(&example("skills/teamwire/SKILL.md"), "### Messages");
    for needed in [
        "A reprompt from you or the user that is longer than 300 characters gets no excerpt",
        "Read the full instruction first, before you act",
        "A send succeeded only when its receipt names a message id and the target role",
        "`NOT SENT`",
        "no message id",
        "A reminder after a member's tool call is not reading",
    ] {
        assert!(
            messages.contains(needed),
            "Messages lacks {needed}: {messages}"
        );
    }
}

#[test]
fn the_skill_says_counts_and_a_clean_verify_are_not_task_success() {
    let skill = example("skills/teamwire/SKILL.md");
    for needed in [
        "A generation succeeded only when `teamwire status` shows the member's result in that generation with the status you need",
        "A message count, a clean `teamwire verify` or a `SKIPPED` check is not task success",
        "An answer in chat or a finding is not a result",
    ] {
        assert!(skill.contains(needed), "the skill lacks {needed}");
    }
}

#[test]
fn the_closer_step_pins_each_check_to_the_verified_sha() {
    let after = section(
        &example("skills/teamwire/SKILL.md"),
        "## After the last wave: integrate, then verify",
    );
    for needed in [
        "teamwire verify <team> --role <role> --pinned-in <clone path>",
        "A branch name is not a commit",
    ] {
        assert!(
            after.contains(needed),
            "the closer step lacks {needed}: {after}"
        );
    }
}

#[test]
fn closing_names_the_checked_close_the_abandon_and_the_bare_close_refusal() {
    let closing = section(
        &example("skills/teamwire/SKILL.md"),
        "## Closing a team down",
    );
    for needed in [
        "teamwire close <team> --expect <role>:<generation>:<status>",
        "teamwire close <team> --abandon \"<why>\"",
        "A bare close refuses while a started member's current result is `blocked` or `failed`",
        "or its current generation has no result after an older one had one",
        "A refused reprompt starts no generation",
        "`skipped` and `clean` are not results",
        "Never use `--abandon` to get past a refused send",
    ] {
        assert!(
            closing.contains(needed),
            "Closing lacks {needed}: {closing}"
        );
    }
}

#[test]
fn the_reference_close_steps_hold_the_outcome_check() {
    let close = section(&example("skills/teamwire/reference.md"), "## Close");
    for needed in [
        "It refuses an outcome you have not accepted",
        "`--expect role:generation:status`",
        "`--abandon WHY`",
        "re-checks the outcomes under the close lock",
    ] {
        assert!(close.contains(needed), "Close lacks {needed}: {close}");
    }
}
