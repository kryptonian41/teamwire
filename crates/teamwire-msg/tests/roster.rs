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

use teamwire_core::{MemberKind, MemberState, MessageType};
use teamwire_msg::roster;

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
    assert_eq!(
        accepts("impl"),
        [
            MessageType::Notice,
            MessageType::Finding,
            MessageType::Answer
        ]
    );
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
