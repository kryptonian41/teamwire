//! `roster`: what `team_members` and `teamwire members` return per member
//! kind. The field names carry their meaning, because a caller sees only
//! the names.
#![expect(
    clippy::indexing_slicing,
    reason = "a test indexes the JSON it checks; a failure there fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use teamwire_core::{MemberKind, MemberState, MessageType, TicketRef};
use teamwire_msg::{describe, roster};

use rig::Rig;

#[test]
fn each_kind_lists_what_it_accepts_from_members() {
    let mut rig = Rig::new();
    rig.add_member("impl", MemberKind::Implementer);
    rig.add_member("scout", MemberKind::Investigator);
    rig.add_member("rev", MemberKind::Reviewer);
    rig.add_member("close", MemberKind::Closer);
    let entries = roster(&rig.ctx()).unwrap();
    let accepts = |name: &str| {
        entries
            .iter()
            .find(|e| e.role.as_str() == name)
            .unwrap()
            .accepts_from_members
            .clone()
    };
    // Every agent that talks takes every kind of information.
    for talker in ["impl", "scout"] {
        assert_eq!(
            accepts(talker),
            [
                MessageType::Notice,
                MessageType::Finding,
                MessageType::Question,
                MessageType::Answer
            ],
            "{talker}"
        );
    }
    assert_eq!(
        accepts("scout"),
        [
            MessageType::Notice,
            MessageType::Finding,
            MessageType::Question,
            MessageType::Answer
        ]
    );
    // A reviewer and a closer take messages only from the lead.
    assert!(accepts("rev").is_empty());
    assert!(accepts("close").is_empty());
    // The lead's own types are never in the list: it is about members.
    for e in &entries {
        assert!(!e.accepts_from_members.contains(&MessageType::Note));
        assert!(!e.accepts_from_members.contains(&MessageType::Reprompt));
    }
}

/// A plan reviewer is quiet like a reviewer: only the lead messages it.
#[test]
fn a_plan_reviewer_accepts_nothing_from_members() {
    let mut rig = Rig::new();
    rig.add_member("impl", MemberKind::Implementer);
    rig.add_member("plan", MemberKind::PlanReviewer);
    let entries = roster(&rig.ctx()).unwrap();
    let plan = entries.iter().find(|e| e.role.as_str() == "plan").unwrap();
    assert_eq!(plan.kind, MemberKind::PlanReviewer);
    assert!(plan.accepts_from_members.is_empty());
    let json = serde_json::to_value(plan).unwrap();
    assert_eq!(json["kind"], "plan_reviewer");
    assert_eq!(json["accepts_from_members"], serde_json::json!([]));
}

/// A reconciler is quiet by its kind, and an investigator whose spec set
/// `quiet` is quiet by its setting: neither accepts anything from members,
/// and the JSON says `quiet` true for both, as for a reviewer, whose stored
/// setting is false. A reader of the JSON needs no list of quiet kinds.
#[test]
fn the_json_says_who_is_quiet_in_effect_and_names_the_set() {
    let mut rig = Rig::new();
    rig.add_member("impl", MemberKind::Implementer);
    rig.add_member("rev", MemberKind::Reviewer);
    rig.add_member("rec", MemberKind::Reconciler);
    rig.add_member("hush", MemberKind::Investigator);
    rig.store
        .write(|tx| {
            tx.set_member_marks(&rig::role("rev"), false, Some("rev1"))?;
            tx.set_member_marks(&rig::role("hush"), true, Some("rev1"))
        })
        .unwrap();
    let entries = roster(&rig.ctx()).unwrap();
    let json = |name: &str| {
        serde_json::to_value(entries.iter().find(|e| e.role.as_str() == name).unwrap()).unwrap()
    };
    for (name, quiet, set) in [
        ("impl", false, serde_json::Value::Null),
        ("rev", true, serde_json::json!("rev1")),
        ("rec", true, serde_json::Value::Null),
        ("hush", true, serde_json::json!("rev1")),
    ] {
        let entry = json(name);
        assert_eq!(entry["quiet"], quiet, "{name}");
        assert_eq!(entry["set"], set, "{name}");
        let accepts_nothing = entry["accepts_from_members"] == serde_json::json!([]);
        assert_eq!(accepts_nothing, quiet, "{name}");
    }
    assert_eq!(json("rec")["kind"], "reconciler");
    assert_eq!(json("hush")["kind"], "investigator");
}

#[test]
fn the_json_names_the_lifecycle_and_not_a_live_state() {
    let mut rig = Rig::new();
    rig.add_member("scout", MemberKind::Investigator);
    let entries = roster(&rig.ctx()).unwrap();
    assert_eq!(entries[0].lifecycle, MemberState::Started);
    let json = serde_json::to_value(&entries[0]).unwrap();
    assert_eq!(json["lifecycle"], "started");
    assert!(json["accepts_from_members"].is_array());
    assert!(json.get("state").is_none(), "{json}");
    assert!(json.get("accepts").is_none(), "{json}");
}

fn linear(id: &str) -> TicketRef {
    TicketRef {
        provider: "linear".to_owned(),
        id: id.to_owned(),
    }
}

fn github(id: &str) -> TicketRef {
    TicketRef {
        provider: "github".to_owned(),
        id: id.to_owned(),
    }
}

#[test]
fn a_team_entry_carries_its_refs_in_the_json() {
    let mut rig = Rig::new();
    rig.add_member("scout", MemberKind::Investigator);
    assert!(describe(&rig.ctx()).unwrap().refs.is_empty());
    let json = serde_json::to_value(describe(&rig.ctx()).unwrap()).unwrap();
    assert_eq!(json["refs"], serde_json::json!([]));

    rig.store
        .write(|tx| tx.set_refs(&[linear("GKA-580"), github("#1")]))
        .unwrap();
    let json = serde_json::to_value(describe(&rig.ctx()).unwrap()).unwrap();
    assert_eq!(
        json["refs"],
        serde_json::json!([
            {"provider": "linear", "id": "GKA-580"},
            {"provider": "github", "id": "#1"},
        ])
    );
}
