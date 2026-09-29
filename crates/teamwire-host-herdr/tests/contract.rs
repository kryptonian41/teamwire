//! The shared host rules, against `Herdr` over `fake-herdr`.

#[path = "support/helpers.rs"]
mod helpers;

use helpers::{T, World};
use teamwire_core::{Address, Host, SpawnRequest};
use teamwire_fakes::contract::{HostSetup, host_contract};

#[test]
fn herdr_passes_the_host_contract() {
    let world = World::new();
    let request = |name: &Address| -> SpawnRequest { world.request(name.as_str(), "contract") };
    let mut setup = HostSetup::new(&world.herdr, &request);
    setup.echoes_input = true;
    setup.timeout = T;
    host_contract(&setup);
    // The suite leaves nothing behind.
    assert!(
        !world.calls_of("agent start").is_empty(),
        "the suite spawned through herdr"
    );
    assert!(world.herdr.live_agent_names(T).unwrap().is_empty());
}
