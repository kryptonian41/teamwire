//! The set warnings of LCH-34: what launch says about the readers of a set
//! and about a reconciler, for members of the spec file and for members the
//! team already holds. Every rule warns; none refuses.

#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use common::{Rig, pane_member, team_spec};
use serde_json::{Value, json};
use teamwire_core::{MemberState, Role};
use teamwire_store::Store;

const CLAUDE: &str = "claude-opus-5-5";
const GPT: &str = "gpt-6.1-sol";
const GEMINI: &str = "gemini-4-carbon";

/// A quiet reviewer in set `set` on `model`, with the shared brief.
fn reader(role: &str, model: Option<&str>, set: &str, cwd: &std::path::Path) -> Value {
    let mut m = common::with(pane_member(role, role, cwd), "kind", json!("reviewer"));
    m = common::with(m, "set", json!(set));
    m = common::with(m, "task", json!("review the diff"));
    if let Some(model) = model {
        m = common::with(m, "model", json!(model));
    }
    m
}

/// The set warnings of a validation that passed.
fn set_warnings(rig: &Rig, children: &[Value], add: bool) -> Vec<String> {
    let plan = rig.validate(&team_spec("demo", children), add).unwrap();
    plan.warnings
        .into_iter()
        .filter(|w| w.starts_with("set_"))
        .collect()
}

/// Launches `children` as the first wave of team `demo`.
fn launch(rig: &Rig, children: &[Value]) -> Store {
    let plan = rig.validate(&team_spec("demo", children), false).unwrap();
    rig.launch("demo", plan).1
}

fn fail(rig: &Rig, store: &mut Store, role: &str) {
    use teamwire_core::Clock as _;
    let at = rig.clock.now();
    let role = Role::parse(role).unwrap();
    store
        .write(|tx| tx.set_member_state(at, &role, MemberState::Failed, &json!({})))
        .unwrap();
}

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

/// Example 1 of the brief: `reva` fails and never reports, and its
/// replacement of the same family prints no `set_same_family`.
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
