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

/// Each rule that allowed one reviewer, one review or no second review keeps
/// its meaning and gains the set: one review round, which may be one reader
/// or one set, and never a second round. Each place a single reader runs
/// says where a set takes its place.
#[test]
fn the_rules_of_one_review_round_gain_the_set() {
    let skill = flat(&example("skills/teamwire/SKILL.md"));
    for phrase in [
        // The shapes table.
        "one review round reads the finished diff: one fresh reviewer, or one reviewer set.",
        "Never a second review round on the same diff.",
        // "The run in order".
        "2. Plan reviewer, or a plan-reviewer set the user chose,",
        "One review round reads each finished diff: one reviewer, or a reviewer set the user chose.",
        // "Have a fresh member read the plan", "Once per plan".
        "the set takes the place of the plan reviewer, from `teamwire preset plan-review-set`",
        "One review round reads each plan: one plan reviewer, or one plan-reviewer set.",
        "Never review the same plan twice, with one reviewer or with a set.",
        // "Settle the open questions".
        "Ask whether the team uses a plan-reviewer set or a reviewer set",
        // "After the last wave", the start and step 3.
        "That round is one reviewer, or one reviewer set and its reconciler.",
        "Allow one review round, by one fresh reviewer or one reviewer set, then at most \
         two fix rounds by the owner, and never a second review round on the same diff.",
    ] {
        assert!(skill.contains(phrase), "the skill lacks: {phrase}");
    }
    for gone in [
        "one fresh reviewer reads the finished diff, then",
        "Allow one fresh review, then",
    ] {
        assert!(!skill.contains(gone), "the skill still says: {gone}");
    }
}

/// The spec fields and the roles name `set` and `quiet`. The setting is
/// `quiet`, never `silent`: the watch already prints `SILENT <role>` for a
/// member that stopped answering.
#[test]
fn the_skill_describes_set_and_quiet() {
    let raw = example("skills/teamwire/SKILL.md");
    let skill = flat(&raw);
    for phrase in [
        "- `set` puts a member in a set of readers",
        "- `quiet`: `\"quiet\": true` makes a member of any kind send no message",
        "The `quiet` setting makes a member of any kind quiet",
        "launch refuses `\"quiet\": false` on one of them.",
        "A planner in a later planner set is an `investigator` with `\"quiet\": true`",
        "Reviewers, plan reviewers, reconcilers, closers and every member with \
         `\"quiet\": true` send nothing",
        "every Claude Code reviewer, plan reviewer, reconciler, closer and investigator this map",
        "| reader of a set | Once the set's reconciler has reported.",
        "| reconciler | Once you have read its report and sent each owner its section. |",
        "The `family` column names each model's family",
    ] {
        assert!(skill.contains(phrase), "the skill lacks: {phrase}");
    }
    // The watch line `SILENT` is upper case, so the setting's name is
    // matched as written and the phrase in any case.
    for setting in ["`silent`", "\"silent\""] {
        assert!(!raw.contains(setting), "the skill names {setting}");
    }
    assert!(!raw.to_lowercase().contains("silent setting"));
}

/// The closing section and the reference show the money line, with a
/// made-up role and the model as `<provider>/<model>`, since no file under
/// `examples/` names a real model.
#[test]
fn the_skill_and_the_reference_show_the_money_line() {
    let line = "teamwire.member.cost{team=myteam,role=revb,kind=reviewer,\
                model=<provider>/<model>,effort=high,task_class=judgement} 1.5878 usd";
    let skill = example("skills/teamwire/SKILL.md");
    let reference = example("skills/teamwire/reference.md");
    let (_, closing) = skill.split_once("## Closing a team down").unwrap();
    assert!(closing.contains(line), "{closing}");
    assert!(flat(closing).contains("read a missing line as not recorded, never as zero"));
    assert!(reference.contains(line));
    assert!(reference.contains("The counter `teamwire.member.cost`, unit `usd`"));
}

/// The reference lists every set warning launch prints, as the interface
/// doc names them, so a lead that meets one knows what to do.
#[test]
fn the_reference_lists_every_set_warning() {
    let cli = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/api/cli.md"),
    )
    .unwrap();
    let codes: Vec<&str> = cli
        .lines()
        .filter_map(|l| l.strip_prefix("| `set_"))
        .filter_map(|rest| rest.split('`').next())
        .collect();
    assert_eq!(codes.len(), 8, "{codes:?}");
    let reference = example("skills/teamwire/reference.md");
    let (_, section) = reference
        .split_once("## Launch warnings for a set")
        .unwrap();
    let (section, _) = section.split_once("\n## ").unwrap();
    for code in codes {
        assert!(
            section.contains(&format!("| `set_{code}` |")),
            "the reference lacks set_{code}"
        );
    }
    assert!(
        reference
            .contains("- launch prints a `warning: set_` line: \"Launch warnings for a set\";")
    );
}

/// An away-from-keyboard run uses a set only when the task text asks, and
/// the seat table covers every member of a set.
#[test]
fn the_afk_skill_keeps_a_set_off_unless_the_task_asks() {
    let afk = flat(&example("skills/afk/SKILL.md"));
    for phrase in [
        "A set of plan reviewers or of reviewers is off in an AFK run, unless the user's \
         task text asks for one.",
        "the plan reviewer or reviewer row covers each reader of the set, and the \
         reconciler row covers the member that joins their reports.",
        "| reconciler | reconciler, opus, xhigh |",
        "write each pick, whether it has a measured result for its seat, and its \
         estimated money into `afk/decisions.md`",
    ] {
        assert!(afk.contains(phrase), "the AFK skill lacks: {phrase}");
    }
}

/// Every place that counts the presets says six shapes and five steps, and
/// the README's table names every preset.
#[test]
fn every_count_of_the_presets_says_six_shapes_and_five_steps() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| flat(&std::fs::read_to_string(root.join(path)).unwrap());
    for path in [
        "examples/README.md",
        "docs/requirements.md",
        "docs/conventions.md",
        "docs/api/cli.md",
    ] {
        let text = read(path);
        assert!(
            text.contains("six team shapes and the five steps")
                || text.contains("six team shapes and five steps"),
            "{path} does not count five steps"
        );
        assert!(
            !text.contains("three steps"),
            "{path} still counts three steps"
        );
    }
    let readme = read("README.md");
    for name in super::NAMES {
        assert!(
            readme.contains(&format!("| `{name}` |")),
            "README.md does not list {name}"
        );
    }
}

fn repo_doc(path: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path),
    )
    .unwrap()
}

/// The research note quotes, word for word, each sentence of the older note
/// that is no longer true, and leaves the older note as it was.
#[test]
fn the_research_note_quotes_the_older_note_word_for_word() {
    let note = repo_doc("docs/research/2026-11-19-model-family-sets.md");
    let older = flat(&repo_doc(
        "docs/research/2026-11-03-elephant-goldfish-model.md",
    ));
    let (_, section) = note
        .split_once(
            "## 7. Sentences in the 2026-11-03 elephant-goldfish note that are no longer true",
        )
        .unwrap();
    let quotes: Vec<&str> = section
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("> "))
        .collect();
    assert!(quotes.len() >= 10, "{} quotes", quotes.len());
    for quote in quotes {
        assert!(
            older.contains(&flat(quote)),
            "not in the older note: {quote}"
        );
    }
}

/// The decision record states the twelve rules, adds to ADR 0034 and ADR
/// 0053, and supersedes nothing.
#[test]
fn the_decision_record_states_the_rules_of_a_set() {
    let adr = flat(&repo_doc(
        "docs/adr/0061-sets-of-readers-on-several-model-families.md",
    ));
    for phrase in [
        "Supersedes no record. Adds to ADR 0034",
        "and to ADR 0053",
        "A set is one review round, not a second review.",
        "The repository names no model for a set.",
        "No source measured a third reader",
        "No source built and tested a joined plan.",
        "A planner set is a later step",
        "docs/research/2026-11-19-model-family-sets.md",
    ] {
        assert!(adr.contains(phrase), "ADR 0061 lacks: {phrase}");
    }
    for rule in 1..=12 {
        assert!(
            adr.contains(&format!(" {rule}. ")),
            "ADR 0061 lacks rule {rule}"
        );
    }
}
