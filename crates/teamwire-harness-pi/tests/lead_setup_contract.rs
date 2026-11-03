//! The shared lead-setup contract suite against `Pi`, with pi's directory
//! and teamwire's state directory in separate temporary directories.

use std::sync::Arc;

use teamwire_core::LeadCommand;
use teamwire_fakes::contract::{LeadSetupSetup, lead_setup_contract};
use teamwire_fakes::{FakePlatform, TestHome};
use teamwire_harness_pi::{Pi, PiConfig};

const SHARED: &str = "---\nname: teamwire\ndescription: d\n---\n\n# Body\n";

#[test]
fn pi_passes_the_lead_setup_contract() {
    let agent_dir = TestHome::new().unwrap();
    let home = TestHome::new().unwrap();
    let platform = Arc::new(FakePlatform::new());
    let pi = Pi::new(
        platform.clone(),
        PiConfig {
            agent_dir: agent_dir.path().to_path_buf(),
            pi_bin: "pi".to_owned(),
        },
    );
    lead_setup_contract(&LeadSetupSetup {
        setup: &pi,
        platform: &platform,
        home: home.path(),
        command: LeadCommand {
            bin: "/opt/bin/teamwire".into(),
            home: Some(home.path().to_path_buf()),
        },
        shared_skill: SHARED,
        before_read: &|_| {},
    });
}
