//! Helpers shared by the set tests: readers and reconcilers as spec
//! entries, the set warnings of a validation, and the store changes that
//! stand for a reader that reports, fails, is removed or is reprompted.

use serde_json::{Value, json};
use teamwire_core::{CompletionStatus, FinalSource, MemberState, Role};
use teamwire_store::{NewCompletion, Store};

use crate::common::{self, Rig, pane_member, team_spec};

pub(crate) const CLAUDE: &str = "claude-opus-5-5";
pub(crate) const GPT: &str = "gpt-6.1-sol";
pub(crate) const GEMINI: &str = "gemini-4-carbon";

/// A quiet reviewer in set `set` on `model`, with the shared brief.
pub(crate) fn reader(role: &str, model: Option<&str>, set: &str, cwd: &std::path::Path) -> Value {
    let mut m = common::with(pane_member(role, role, cwd), "kind", json!("reviewer"));
    m = common::with(m, "set", json!(set));
    m = common::with(m, "task", json!("review the diff"));
    if let Some(model) = model {
        m = common::with(m, "model", json!(model));
    }
    m
}

pub(crate) fn reconciler(role: &str, set: Option<&str>, cwd: &std::path::Path) -> Value {
    let m = common::with(pane_member(role, role, cwd), "kind", json!("reconciler"));
    match set {
        Some(set) => common::with(m, "set", json!(set)),
        None => m,
    }
}

/// The set warnings of a validation that passed.
pub(crate) fn set_warnings(rig: &Rig, children: &[Value], add: bool) -> Vec<String> {
    let plan = rig.validate(&team_spec("demo", children), add).unwrap();
    plan.warnings
        .into_iter()
        .filter(|w| w.starts_with("set_"))
        .collect()
}

/// Launches `children` as the first wave of team `demo`.
pub(crate) fn launch(rig: &Rig, children: &[Value]) -> Store {
    let plan = rig.validate(&team_spec("demo", children), false).unwrap();
    rig.launch("demo", plan).1
}

/// Stores a completion of `generation` for `role`.
pub(crate) fn complete(rig: &Rig, store: &mut Store, role: &str, generation: u32) {
    use teamwire_core::Clock as _;
    let at = rig.clock.now();
    store
        .write(|tx| {
            tx.insert_completion(
                at,
                &NewCompletion {
                    role: Role::parse(role).unwrap(),
                    generation,
                    status: CompletionStatus::Done,
                    summary: "reviewed".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
            .map(|_| ())
        })
        .unwrap();
}

pub(crate) fn fail(rig: &Rig, store: &mut Store, role: &str) {
    use teamwire_core::Clock as _;
    let at = rig.clock.now();
    let role = Role::parse(role).unwrap();
    store
        .write(|tx| tx.set_member_state(at, &role, MemberState::Failed, &json!({})))
        .unwrap();
}

pub(crate) fn remove(rig: &Rig, store: &mut Store, role: &str) {
    use teamwire_core::Clock as _;
    let at = rig.clock.now();
    let role = Role::parse(role).unwrap();
    store
        .write(|tx| tx.remove_member(at, &role, None, &json!({})).map(|_| ()))
        .unwrap();
}

pub(crate) fn reprompt(rig: &Rig, store: &mut Store, role: &str) {
    use teamwire_core::Clock as _;
    let at = rig.clock.now();
    let role = Role::parse(role).unwrap();
    store
        .write(|tx| tx.bump_generation(at, &role, "m001").map(|_| ()))
        .unwrap();
}

/// Three reviewers of three families in set `rev1`.
pub(crate) fn three_readers(cwd: &std::path::Path) -> [Value; 3] {
    [
        reader("reva", Some(GPT), "rev1", cwd),
        reader("revb", Some(CLAUDE), "rev1", cwd),
        reader("revc", Some(GEMINI), "rev1", cwd),
    ]
}
