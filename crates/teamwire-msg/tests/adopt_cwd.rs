//! `lead --adopt` records the folder of the new lead session, so `reopen`
//! can resume that session from where it runs.

#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#[expect(dead_code, reason = "this file uses part of the shared rig")]
#[path = "support/rig.rs"]
mod rig;

use std::path::PathBuf;

use teamwire_msg::CallerEnv;

use rig::{Rig, sid};

#[test]
fn adopting_records_the_new_leads_folder() {
    let mut rig = Rig::new();
    rig.harness.set_interactive_caller(sid("new-lead"));
    rig.harness.set_session_cwd(sid("new-lead"), "/work/new");
    let env = CallerEnv {
        from: Some("lead".to_owned()),
        ..CallerEnv::default()
    };
    teamwire_msg::adopt_lead(&mut rig.ctx(), &env).unwrap();
    let cwd = rig.store.read(|tx| tx.lead_cwd()).unwrap();
    assert_eq!(cwd, Some(PathBuf::from("/work/new")));
}
