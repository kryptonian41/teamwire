//! The shared lead-setup contract suite against `ClaudeCode`, with Claude's
//! files and teamwire's state directory in separate temporary directories.

use std::sync::Arc;

use teamwire_core::LeadCommand;
use teamwire_fakes::contract::{LeadSetupSetup, lead_setup_contract};
use teamwire_fakes::{FakePlatform, TestHome};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig, SKILL_TEXT};

#[test]
fn claude_code_passes_the_lead_setup_contract() {
    let claude_dir = TestHome::new().unwrap();
    let home = TestHome::new().unwrap();
    let platform = Arc::new(FakePlatform::new());
    let claude = ClaudeCode::new(
        platform.clone(),
        ClaudeConfig {
            claude_dir: claude_dir.path().to_path_buf(),
            claude_json: claude_dir.path().join(".claude.json"),
            claude_bin: "claude".to_owned(),
        },
    );
    lead_setup_contract(&LeadSetupSetup {
        setup: &claude,
        platform: &platform,
        home: home.path(),
        command: LeadCommand {
            bin: "/opt/bin/teamwire".into(),
            home: Some(home.path().to_path_buf()),
        },
        shared_skill: SKILL_TEXT,
        before_read: &|_| {},
    });
}
