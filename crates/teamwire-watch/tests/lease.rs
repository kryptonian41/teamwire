//! One watch per team (WAT-16).

#[path = "support/fixture.rs"]
pub mod fixture;

use fixture::Bed;
use teamwire_core::Pid;
use teamwire_watch::{WatchError, WatchMemory};

fn start(bed: &mut Bed, memory: &mut WatchMemory, pid: u32) -> Result<(), WatchError> {
    let mut ctx = teamwire_store::Ctx {
        home: bed.home.path(),
        store: &mut bed.store,
        registry: &bed.registry,
        platform: &bed.platform,
        clock: &bed.clock,
        config: &bed.config,
    };
    teamwire_watch::start(&mut ctx, memory, Pid::new(pid), "testhost")
}

fn stop(bed: &mut Bed, memory: &mut WatchMemory) {
    let mut ctx = teamwire_store::Ctx {
        home: bed.home.path(),
        store: &mut bed.store,
        registry: &bed.registry,
        platform: &bed.platform,
        clock: &bed.clock,
        config: &bed.config,
    };
    teamwire_watch::stop(&mut ctx, memory).unwrap_or_else(|e| unreachable!("{e}"));
}

#[test]
fn second_watch_refused() {
    let mut bed = Bed::new();
    bed.platform.set_alive(Pid::new(111), true);
    let mut first = WatchMemory::default();
    start(&mut bed, &mut first, 111).unwrap_or_else(|e| unreachable!("{e}"));

    let mut second = WatchMemory::default();
    let refused = start(&mut bed, &mut second, 222);
    assert_eq!(
        refused,
        Err(WatchError::Held {
            pid: Pid::new(111),
            host: "testhost".to_owned()
        })
    );
    assert!(refused.unwrap_err().to_string().contains("111"));
}

#[test]
fn a_dead_holder_is_taken_over() {
    let mut bed = Bed::new();
    bed.platform.set_alive(Pid::new(111), true);
    let mut first = WatchMemory::default();
    start(&mut bed, &mut first, 111).unwrap_or_else(|e| unreachable!("{e}"));
    bed.platform.set_alive(Pid::new(111), false);

    let mut second = WatchMemory::default();
    start(&mut bed, &mut second, 222).unwrap_or_else(|e| unreachable!("{e}"));
    let lease = bed.store.read(|tx| tx.watch_lease()).unwrap_or_default();
    assert_eq!(lease.map(|l| l.pid), Some(Pid::new(222)));
}

#[test]
fn the_lease_is_released_and_the_old_watch_stops_when_taken_over() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.working("api");
    bed.platform.set_alive(Pid::new(111), true);
    let mut first = WatchMemory::default();
    start(&mut bed, &mut first, 111).unwrap_or_else(|e| unreachable!("{e}"));
    bed.memory = first;
    assert!(bed.poll().is_some());
    // The first process is presumed dead and another takes the lease.
    bed.platform.set_alive(Pid::new(111), false);
    let mut second = WatchMemory::default();
    start(&mut bed, &mut second, 222).unwrap_or_else(|e| unreachable!("{e}"));
    // The first one, in fact alive, polls again: it must stop, silently.
    assert!(bed.tick().is_none());
    assert_eq!(bed.memory.end, Some(teamwire_watch::WatchEnd::LeaseLost));

    stop(&mut bed, &mut second);
    let lease = bed.store.read(|tx| tx.watch_lease()).unwrap_or_default();
    assert!(lease.is_none());
}
