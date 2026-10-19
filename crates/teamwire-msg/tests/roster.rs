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
