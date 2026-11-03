//! ADR 0028: a sender that would record `undelivered` first reads whether
//! the recipient's turn ended during the attempt. That stop left the message
//! to the sender, so the sender tries once more instead of losing it.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a failing unwrap or index in an integration test fails the test"
)]

#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use teamwire_core::{
    Address, Clock as _, DeliveryErrorCode, DeliveryState, EventType, Host, HostCaps, HostError,
    HostId, Key, MemberKind, MessageType, NoHost, Occupant, OneLine, Place, PlaceState, Registry,
    SpawnRequest, TeamName,
};
use teamwire_fakes::{FakeClock, FakeHost};
use teamwire_msg::{RecipientState, send};
use teamwire_store::Store;

use rig::{Rig, to};

/// The fake host, with `before` run ahead of every `read_tail`.
struct ReadHook {
    inner: Arc<FakeHost>,
    before: Box<dyn Fn() + Send + Sync>,
}

impl Host for ReadHook {
    fn id(&self) -> HostId {
        self.inner.id()
    }
    fn capabilities(&self) -> HostCaps {
        self.inner.capabilities()
    }
    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError> {
        self.inner.spawn(req, timeout)
    }
    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError> {
        self.inner.submit(place, text, timeout)
    }
    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
        self.inner.state(place, timeout)
    }
    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError> {
        self.inner.occupant(place, timeout)
    }
    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError> {
        (self.before)();
        self.inner.read_tail(place, lines, timeout)
    }
    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError> {
        self.inner.send_keys(place, keys, timeout)
    }
    fn close(&self, place: &Place, expected: &Address, timeout: Duration) -> Result<(), HostError> {
        self.inner.close(place, expected, timeout)
    }
    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
        self.inner.list(timeout)
    }
    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
        self.inner.live_agent_names(timeout)
    }
    fn place_env_var(&self) -> Option<&'static str> {
        self.inner.place_env_var()
    }
    fn place_from_env(&self, read: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        self.inner.place_from_env(read)
    }
}

/// What the recipient's turn-end hook writes when it finds its delivery
/// still `pending`, through a connection of its own, as the hook's process
/// would.
fn recipient_stops(home: &std::path::Path, clock: &FakeClock) {
    let mut store = Store::open(home, &TeamName::parse("alpha").unwrap()).unwrap();
    let api = to("api");
    let at = clock.now();
    store
        .write(|tx| {
            let rows = tx
                .reader()
                .deliveries_for(&api, &[DeliveryState::Pending])?;
            for d in &rows.items {
                tx.record_recipient_stopped(at, &d.delivery.message, &api)?;
            }
            Ok(())
        })
        .unwrap();
}

/// What a read hook may touch: the rig's files, clock, host and `api`'s place.
struct View {
    home: PathBuf,
    clock: Arc<FakeClock>,
    host: Arc<FakeHost>,
    api: Place,
}

/// `api` runs a turn whose screen never shows the input prompt, on a host
/// that does not guard dialogs, so each attempt reads the screen three
/// times and gives up. `on_read(n)` runs before the `n`th read (from 1).
fn busy_api(
    on_read: impl Fn(usize, &View) + Send + Sync + 'static,
) -> (Rig, Place, Arc<AtomicUsize>) {
    let mut rig = Rig::new();
    rig.add_member("scout", MemberKind::Investigator);
    let api = rig.add_member("api", MemberKind::Implementer);
    rig.host.set_caps(HostCaps {
        guards_dialogs: false,
        ..FakeHost::caps()
    });
    rig.host.set_screen(&api, "working on the api");
    let reads = Arc::new(AtomicUsize::new(0));
    let hook = ReadHook {
        inner: rig.host.clone(),
        before: Box::new({
            let reads = reads.clone();
            let view = View {
                home: rig.home.path().to_path_buf(),
                clock: rig.clock.clone(),
                host: rig.host.clone(),
                api: api.clone(),
            };
            move || on_read(reads.fetch_add(1, Ordering::SeqCst) + 1, &view)
        }),
    };
    let mut registry = Registry::new();
    registry.add_harness(rig.harness.clone()).unwrap();
    registry.add_host(Arc::new(hook)).unwrap();
    registry.add_host(Arc::new(NoHost)).unwrap();
    rig.registry = registry;
    (rig, api, reads)
}

#[test]
fn a_stop_during_the_attempt_makes_the_sender_try_again() {
    let (mut rig, api, reads) = busy_api(|n, view| {
        if n == 3 {
            // The last read of the first attempt: the turn ends meanwhile.
            recipient_stops(&view.home, &view.clock);
        } else if n == 4 {
            view.host.set_screen(&view.api, "done\n> ");
        }
    });
    let req = rig.request("scout", &["api"], MessageType::Question, "which schema?");

    let report = send(&mut rig.ctx(), req).unwrap();

    assert_eq!(report.deliveries[0].state, RecipientState::HandedOff);
    assert_eq!(reads.load(Ordering::SeqCst), 4);
    assert_eq!(rig.host.submitted(&api).len(), 1);
    assert!(rig.events(EventType::DeliveryUndelivered).is_empty());
}

#[test]
fn without_a_stop_the_sender_records_undelivered_as_before() {
    let (mut rig, _, reads) = busy_api(|_, _| {});
    let req = rig.request("scout", &["api"], MessageType::Question, "which schema?");

    let report = send(&mut rig.ctx(), req).unwrap();

    assert_eq!(report.deliveries[0].state, RecipientState::Undelivered);
    assert_eq!(
        report.deliveries[0].reason,
        Some(DeliveryErrorCode::NotAtInputPrompt)
    );
    assert_eq!(reads.load(Ordering::SeqCst), 3);
}

/// A recipient that stops again during every new try (each stop held it for
/// one more turn) cannot keep the sender trying for ever.
#[test]
fn stops_during_every_try_end_in_undelivered_after_a_bounded_number_of_tries() {
    let (mut rig, _, reads) = busy_api(|n, view| {
        if n % 3 == 0 {
            recipient_stops(&view.home, &view.clock);
        }
    });
    let req = rig.request("scout", &["api"], MessageType::Question, "which schema?");

    let report = send(&mut rig.ctx(), req).unwrap();

    assert_eq!(report.deliveries[0].state, RecipientState::Undelivered);
    // The first attempt and three more.
    assert_eq!(reads.load(Ordering::SeqCst), 12);
}
