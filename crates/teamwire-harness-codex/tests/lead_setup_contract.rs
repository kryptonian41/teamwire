//! The shared lead-setup contract suite against `CodexCli`. Codex is asked
//! what is installed with `codex plugin list --json`, so every read is
//! answered with an empty list.

use std::sync::Arc;

use teamwire_core::{LeadCommand, RunOutput};
use teamwire_fakes::contract::{LeadSetupSetup, lead_setup_contract};
use teamwire_fakes::{FakePlatform, TestHome};
use teamwire_harness_codex::{CodexCli, CodexConfig};

const SHARED: &str = "---\nname: teamwire\ndescription: d\n---\n\n# Body\n";

fn nothing_installed(platform: &FakePlatform) {
    platform.queue_run(Ok(RunOutput {
        exit_code: 0,
        stdout: br#"{"installed":[],"available":[]}"#.to_vec(),
        stderr: Vec::new(),
        timed_out: false,
    }));
}

#[test]
fn codex_passes_the_lead_setup_contract() {
    let codex_home = TestHome::new().unwrap();
    let home = TestHome::new().unwrap();
    let platform = Arc::new(FakePlatform::new());
    let codex = CodexCli::new(
        platform.clone(),
        CodexConfig {
            codex_home: codex_home.path().to_path_buf(),
            codex_bin: "codex".to_owned(),
        },
    );
    lead_setup_contract(&LeadSetupSetup {
        setup: &codex,
        platform: &platform,
        home: home.path(),
        command: LeadCommand {
            bin: "/opt/bin/teamwire".into(),
            home: None,
        },
        shared_skill: SHARED,
        before_read: &nothing_installed,
    });
}
