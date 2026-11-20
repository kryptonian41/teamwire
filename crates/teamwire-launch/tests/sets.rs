//! The set warnings of LCH-34 about the readers of a set: what launch says
//! for members of the spec file and for members the team already holds.
//! Every rule warns; none refuses.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/sets.rs"]
mod sets;

use common::{Rig, pane_member};
use serde_json::{Value, json};
use sets::{CLAUDE, GEMINI, GPT, fail, launch, reader, set_warnings};

#[test]
fn one_reader_is_a_set_too_small() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let warnings = set_warnings(&rig, &[reader("reva", Some(GPT), "rev1", cwd)], false);
    assert_eq!(
        warnings,
        ["set_too_small: set rev1 has one reader (reva); a set needs at least two"]
    );
}

#[test]
fn two_readers_of_one_family_warn() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let warnings = set_warnings(
        &rig,
        &[
            reader("reva", Some(GPT), "rev1", cwd),
            reader("revb", Some("openrouter/openai/o3"), "rev1", cwd),
        ],
        false,
    );
    assert_eq!(
        warnings,
        [
            "set_same_family: set rev1: reva and revb are both gpt models; a set needs a different \
          family per reader"
        ]
    );
}

#[test]
fn a_reader_of_the_spec_file_warns_against_a_reader_the_team_holds() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let store = launch(
        &rig,
        &[
            reader("reva", Some(GPT), "rev1", cwd),
            reader("revb", Some(CLAUDE), "rev1", cwd),
        ],
    );
    drop(store);
    let warnings = set_warnings(&rig, &[reader("revc", Some(GPT), "rev1", cwd)], true);
    assert_eq!(
        warnings,
        [
            "set_same_family: set rev1: reva and revc are both gpt models; a set needs a different \
          family per reader"
        ]
    );
}

#[test]
fn a_reader_with_no_model_or_an_unknown_family_warns() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let warnings = set_warnings(
        &rig,
        &[
            reader("reva", None, "rev1", cwd),
            reader("revb", Some("openrouter/auto"), "rev1", cwd),
            reader("revc", Some(GEMINI), "rev1", cwd),
        ],
        false,
    );
    assert_eq!(
        warnings,
        [
            "set_family_unknown: set rev1: cannot tell the family of reva (no model); check it by eye",
            "set_family_unknown: set rev1: cannot tell the family of revb (openrouter/auto); check \
             it by eye",
        ]
    );
}

#[test]
fn readers_of_two_kinds_warn() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let planner = common::with(
        reader("revb", Some(CLAUDE), "rev1", cwd),
        "kind",
        json!("plan_reviewer"),
    );
    let warnings = set_warnings(
        &rig,
        &[reader("reva", Some(GPT), "rev1", cwd), planner],
        false,
    );
    assert_eq!(
        warnings,
        [
            "set_mixed_kinds: set rev1: reva is a reviewer and revb is a plan_reviewer; the readers \
          of a set share one kind"
        ]
    );
}

#[test]
fn a_reader_that_may_send_messages_warns_and_a_quiet_one_does_not() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let talker = |role: &str, model: &str| {
        common::with(
            reader(role, Some(model), "plan1", cwd),
            "kind",
            json!("investigator"),
        )
    };
    let quiet = common::with(talker("plb", CLAUDE), "quiet", json!(true));
    let warnings = set_warnings(&rig, &[talker("pla", GPT), quiet], false);
    assert_eq!(
        warnings,
        [
            "set_reader_not_quiet: set plan1: pla may send messages; give it \"quiet\": true so the \
          readers stay independent"
        ]
    );
}

#[test]
fn readers_of_one_spec_file_with_different_briefs_warn() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let brief = cwd.join("brief.md");
    common::write(&brief, "review the diff");
    let from_file = common::with(
        common::with(
            reader("revb", Some(CLAUDE), "rev1", cwd),
            "task",
            Value::Null,
        ),
        "task_file",
        json!(brief),
    );
    let other = common::with(
        reader("revc", Some(GEMINI), "rev1", cwd),
        "task",
        json!("review the plan"),
    );
    let warnings = set_warnings(
        &rig,
        &[reader("reva", Some(GPT), "rev1", cwd), from_file, other],
        false,
    );
    assert_eq!(
        warnings,
        [
            "set_briefs_differ: set rev1: reva and revc have different briefs; the readers of a \
             set get the same brief",
            "set_briefs_differ: set rev1: revb and revc have different briefs; the readers of a \
             set get the same brief",
        ]
    );
}

#[test]
fn a_team_with_no_set_label_gets_no_set_warning() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let plain = |role: &str| {
        common::with(
            common::with(pane_member(role, role, cwd), "kind", json!("investigator")),
            "model",
            json!(GPT),
        )
    };
    assert!(set_warnings(&rig, &[plain("a"), plain("b")], false).is_empty());
}

/// `reva` fails and never reports, and the lead replaces it with `revd`
/// of the same family: the pair prints no `set_same_family`.
#[test]
fn a_failed_reader_that_never_reported_counts_for_no_rule() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let mut store = launch(
        &rig,
        &[
            reader("reva", Some(GPT), "rev1", cwd),
            reader("revb", Some(CLAUDE), "rev1", cwd),
            reader("revc", Some(GEMINI), "rev1", cwd),
        ],
    );
    fail(&rig, &mut store, "reva");
    drop(store);
    let warnings = set_warnings(&rig, &[reader("revd", Some(GPT), "rev1", cwd)], true);
    assert!(warnings.is_empty(), "{warnings:?}");
}
