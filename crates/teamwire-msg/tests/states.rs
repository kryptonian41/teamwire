//! The delivery-state property (`docs/test-strategy.md`): whatever happens
//! to a delivery, its state only moves forward, `acked` is final, and a row
//! never goes back to `pending`.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use std::time::Duration;

use proptest::prelude::*;
use teamwire_core::{Actor, DeliveryState, HostError, HostErrorCode, MemberKind, MessageType};
use teamwire_fakes::Fault;
use teamwire_msg::{ack, inbox, resend, send};

use rig::{Rig, role, to};

/// How far along a state is. `unconfirmed` outranks `undelivered`: a line
/// that may have arrived is further along than one known not to have.
fn rank(state: DeliveryState) -> u8 {
    match state {
        DeliveryState::Pending => 0,
        DeliveryState::Undelivered => 1,
        DeliveryState::Unconfirmed => 2,
        DeliveryState::HandedOff => 3,
        DeliveryState::Acked => 4,
    }
}

#[derive(Debug, Clone)]
enum Step {
    /// The next host call fails this way (or succeeds), then a resend.
    Resend(Option<HostErrorCode>),
    /// The recipient's session disappears or comes back, then a resend.
    Reach(bool),
    Inbox,
    Ack,
    Wait,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        prop_oneof![
            Just(None),
            Just(Some(HostErrorCode::Stalled)),
            Just(Some(HostErrorCode::Timeout)),
            Just(Some(HostErrorCode::NotFound)),
            Just(Some(HostErrorCode::BadArgument)),
            Just(Some(HostErrorCode::Other)),
        ]
        .prop_map(Step::Resend),
        any::<bool>().prop_map(Step::Reach),
        Just(Step::Inbox),
        Just(Step::Ack),
        Just(Step::Wait),
    ]
}

fn state(rig: &Rig, message: &teamwire_core::Ulid) -> DeliveryState {
    rig.store
        .read(|tx| tx.delivery(message, &to("api")))
        .unwrap()
        .unwrap()
        .delivery
        .state
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn delivery_states_only_move_forward(
        first in prop_oneof![
            Just(None),
            Just(Some(HostErrorCode::Stalled)),
            Just(Some(HostErrorCode::NotFound)),
        ],
        steps in proptest::collection::vec(step(), 1..12),
    ) {
        let mut rig = Rig::new();
        rig.add_member("api", MemberKind::Implementer);
        rig.add_member("scout", MemberKind::Investigator);
        if let Some(code) = first {
            rig.host.inject("submit", Fault::Fail(HostError::new(code, "scripted")), 1);
        }
        let req = rig.request("scout", &["api"], MessageType::Finding, "the finding");
        let sent = send(&mut rig.ctx(), req).unwrap();
        let mut previous = state(&rig, &sent.id);
        prop_assert_ne!(previous, DeliveryState::Pending);
        for step in steps {
            match step {
                Step::Resend(code) => {
                    if let Some(code) = code {
                        rig.host.inject("submit", Fault::Fail(HostError::new(code, "scripted")), 1);
                    }
                    // A refusal to resend is fine; only the state matters.
                    let _outcome = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None);
                }
                Step::Reach(up) => {
                    rig.set_reachable("api", up);
                    let _outcome = resend(&mut rig.ctx(), &Actor::Lead, &sent.display_id, None);
                }
                Step::Inbox => {
                    inbox(&mut rig.ctx(), &role("api"), false).unwrap();
                }
                Step::Ack => {
                    ack(&mut rig.ctx(), &role("api"), std::slice::from_ref(&sent.display_id)).unwrap();
                }
                Step::Wait => rig.clock.advance(Duration::from_secs(61)),
            }
            let now = state(&rig, &sent.id);
            prop_assert!(rank(now) >= rank(previous), "{previous} -> {now}");
            prop_assert_ne!(now, DeliveryState::Pending);
            if previous == DeliveryState::Acked {
                prop_assert_eq!(now, DeliveryState::Acked);
            }
            previous = now;
        }
    }
}
