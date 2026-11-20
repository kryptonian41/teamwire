//! The lead's text about sets of readers on several model families: the
//! skill's section, the rules a set meets elsewhere in the skill, the
//! reference file and the away-from-keyboard skill. A lead on Claude Code or
//! on pi runs a set from this text and a preset alone, so each rule the
//! section states is pinned here, one phrase per rule.

use super::{example, flat};

/// The skill's section on sets, flat, between its heading and the next.
fn set_section() -> String {
    let skill = flat(&example("skills/teamwire/SKILL.md"));
    let (_, rest) = skill
        .split_once("## Sets: several model families on one judging job")
        .unwrap();
    let (section, _) = rest.split_once("## Skills and plugins in members").unwrap();
    section.to_owned()
}

/// The section sits after "Choose each member's model", because a set's
/// picks follow that section's harness rule, and it states the twelve rules.
#[test]
fn the_skill_states_the_twelve_rules_of_a_set() {
    let skill = example("skills/teamwire/SKILL.md");
    let model = skill.find("## Choose each member's model").unwrap();
    let sets = skill
        .find("## Sets: several model families on one judging job")
        .unwrap();
    let plugins = skill.find("## Skills and plugins in members").unwrap();
    assert!(model < sets && sets < plugins);

    let section = set_section();
    for phrase in [
        "1. A set is opt-in.",
        "Recommend a set when the team has three or more implementers, or when any \
         member's class is `cross_cutting` or `concurrency_data_loss`.",
        "In an away-from-keyboard run, use a set only when the user's task text asks for one.",
        "2. Pick one model per family: the newest top-tier model of each family",
        "The `family` column says which family a model is.",
        "Use at least two families, and three when a third is reachable.",
        "A set has at most three readers.",
        "3. Say in the plan when a pick has no measured result for its seat",
        "The user confirms or swaps each pick.",
        "4. Set each reader's `harness` from its model's family",
        "set `effort` on every reader.",
        "5. Show the user the estimated money",
        "teamwire has no command that gives a model's tier, release date or price.",
        "6. Give every reader the same brief, and no reader a special focus.",
        "7. Launch the reconciler only after every reader has reported.",
        "8. The reconciler reads the reports under role names.",
        "9. Read the reconciler's summary",
        "\"Findings for <role>\"",
        "\"Findings for the lead\" and the \"Paraphrases\" section",
        "Do not read the readers' reports yourself.",
        "10. A set reads once. Never run a set, or a single reviewer, a second time on \
         the same diff or the same plan.",
        "11. When a reader cannot reach its model, carry on if two families remain.",
        "add no reconciler.",
        "12. For a reader on Claude Code, give the plugins map",
        "no memory feature that carries one reader's findings to another.",
        "`teamwire preset plan-review-set`",
        "`teamwire preset review-set`",
        "**The order of work for a plan-reviewer set.**",
        "**The order of work for a reviewer set.**",
        "renames the `set` label, all four roles and the report paths",
        "A planner set is a later step",
    ] {
        assert!(section.contains(phrase), "the set section lacks: {phrase}");
    }
}

/// The worked example is a spec for a reviewer set whose model ids are
/// placeholders: teamwire names no model for a set (ADR 0033).
#[test]
fn the_set_example_names_no_real_model() {
    let section = set_section();
    for placeholder in [
        "\"model\": \"<newest Claude model>\"",
        "\"model\": \"<provider>/<newest GPT model>\"",
        "\"model\": \"<provider>/<newest Gemini model>\"",
        "\"set\": \"revset\"",
        "\"harness\": \"claude-code\"",
        "\"harness\": \"pi\"",
    ] {
        assert!(
            section.contains(placeholder),
            "the example lacks {placeholder}"
        );
    }
    let lower = section.to_lowercase();
    for name in ["opus", "sonnet", "haiku", "fable", "gpt-", "gemini-"] {
        assert!(!lower.contains(name), "the set section names {name}");
    }
}
