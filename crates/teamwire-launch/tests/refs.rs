//! Ticket refs: a first launch records the spec's refs on the team row and in
//! `team.created`; `--add` appends the ones not recorded, with `team.refs_added`.

#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

#[cfg(test)]
#[path = "support/common.rs"]
#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
mod common;

use common::{Rig, codes, pane_member, team_spec, with};
use serde_json::{Value, json};
use teamwire_core::EventType;
use teamwire_store::Store;

/// A spec whose refs are given as `provider:id`.
fn spec_with_refs(rig: &Rig, role: &str, refs: &[&str]) -> Value {
    let refs: Vec<Value> = refs
        .iter()
        .map(|r| {
            let (provider, id) = r.split_once(':').unwrap();
            json!({"provider": provider, "id": id})
        })
        .collect();
    with(
        team_spec("demo", &[pane_member(role, role, rig.work.path())]),
        "refs",
        json!(refs),
    )
}

/// The team row's refs as `provider:id`.
fn refs_of(store: &Store) -> Vec<String> {
    let team = store.read(|tx| tx.team()).unwrap().unwrap();
    team.refs.iter().map(ToString::to_string).collect()
}

fn events_of(store: &Store, kind: EventType) -> Vec<Value> {
    store
        .read(|tx| tx.events_of_type(kind)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .map(|e| e.data)
        .collect()
}

#[test]
fn a_first_launch_records_the_refs_on_the_row_and_in_team_created() {
    let rig = Rig::new();
    let spec = spec_with_refs(
        &rig,
        "a",
        &["linear:GKA-580", "github:#1", "linear:GKA-580"],
    );
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    assert_eq!(refs_of(&store), ["linear:GKA-580", "github:#1"]);
    let created = events_of(&store, EventType::TeamCreated);
    assert_eq!(
        created[0]["refs"],
        json!([
            {"provider": "linear", "id": "GKA-580"},
            {"provider": "github", "id": "#1"},
        ])
    );
    assert!(events_of(&store, EventType::TeamRefsAdded).is_empty());
}

#[test]
fn a_launch_without_refs_records_an_empty_list() {
    let rig = Rig::new();
    let spec = team_spec("demo", &[pane_member("a", "a", rig.work.path())]);
    let (_, store) = rig.launch("demo", rig.validate(&spec, false).unwrap());
    assert!(refs_of(&store).is_empty());
    assert_eq!(
        events_of(&store, EventType::TeamCreated)[0]["refs"],
        json!([])
    );
}

#[test]
fn add_appends_only_the_new_refs_in_one_event() {
    let rig = Rig::new();
    let first = spec_with_refs(&rig, "a", &["linear:GKA-580", "github:#1"]);
    let (_, store) = rig.launch("demo", rig.validate(&first, false).unwrap());
    drop(store);

    // The same id under another provider is a new ref.
    let more = spec_with_refs(&rig, "b", &["github:#1", "github:#2", "gitlab:#1"]);
    let plan = rig.validate(&more, true).unwrap();
    let planned: Vec<String> = plan.new_refs.iter().map(ToString::to_string).collect();
    assert_eq!(
        planned,
        ["github:#2", "gitlab:#1"],
        "the plan names what it would append"
    );
    let (_, store) = rig.launch("demo", plan);
    assert_eq!(
        refs_of(&store),
        ["linear:GKA-580", "github:#1", "github:#2", "gitlab:#1"]
    );
    let added = events_of(&store, EventType::TeamRefsAdded);
    assert_eq!(
        added,
        [json!({ "added": [
            {"provider": "github", "id": "#2"},
            {"provider": "gitlab", "id": "#1"},
        ] })]
    );
}

#[test]
fn add_that_names_no_new_ref_writes_no_event() {
    let rig = Rig::new();
    let first = spec_with_refs(&rig, "a", &["github:#1"]);
    let (_, store) = rig.launch("demo", rig.validate(&first, false).unwrap());
    drop(store);

    for (role, refs) in [("b", &["github:#1"][..]), ("c", &[][..])] {
        let again = spec_with_refs(&rig, role, refs);
        let plan = rig.validate(&again, true).unwrap();
        assert!(plan.new_refs.is_empty());
        let (_, store) = rig.launch("demo", plan);
        assert_eq!(refs_of(&store), ["github:#1"]);
        assert!(events_of(&store, EventType::TeamRefsAdded).is_empty());
        drop(store);
    }
}

#[test]
fn add_over_twenty_refs_in_total_is_refused_and_starts_nobody() {
    let rig = Rig::new();
    let nineteen: Vec<String> = (0..19).map(|n| format!("linear:T-{n}")).collect();
    let nineteen: Vec<&str> = nineteen.iter().map(String::as_str).collect();
    let first = spec_with_refs(&rig, "a", &nineteen);
    let (_, store) = rig.launch("demo", rig.validate(&first, false).unwrap());
    drop(store);
    let spawned = rig.host.log().count_named("spawn");

    let two_new = spec_with_refs(&rig, "b", &["linear:T-0", "linear:N-1", "linear:N-2"]);
    let errors = rig.validate(&two_new, true).unwrap_err();
    assert_eq!(codes(&errors), ["too_many_refs"]);
    assert_eq!(rig.host.log().count_named("spawn"), spawned);

    // One new ref still fits, and a repeat of a recorded one costs nothing.
    let one_new = spec_with_refs(&rig, "b", &["linear:T-0", "linear:N-1"]);
    rig.validate(&one_new, true).unwrap();
}
