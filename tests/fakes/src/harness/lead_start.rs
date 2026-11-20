//! `FakeHarness::lead_start`: a fresh lead's command line built from the
//! request, with the old flags a test chose to drop left out.

use teamwire_core::{HarnessError, LeadStart, LeadStartSpec};

use super::FakeHarness;
use crate::calls::{CallKind, lock};

impl FakeHarness {
    pub(super) fn build_lead_start(&self, req: &LeadStart) -> Result<LeadStartSpec, HarnessError> {
        self.log
            .record("lead_start", CallKind::Read, req.old_session.as_str());
        let mut state = lock(&self.state);
        state.lead_starts.push(req.clone());
        if let Some(err) = &state.lead_start_fault {
            return Err(err.clone());
        }
        let mut argv = vec![
            "fake-claude".to_owned(),
            "--session-id".to_owned(),
            req.new_session.as_str().to_owned(),
        ];
        let mut dropped = Vec::new();
        for word in req.old_args.iter().skip(1) {
            if state.lead_start_drops.contains(word) {
                dropped.push(word.clone());
            } else {
                argv.push(word.clone());
            }
        }
        argv.push(req.prompt.clone());
        Ok(LeadStartSpec {
            argv,
            dropped,
            warnings: state.lead_start_warnings.clone(),
        })
    }

    /// Each request `lead_start` got, oldest first.
    pub fn lead_starts(&self) -> Vec<LeadStart> {
        lock(&self.state).lead_starts.clone()
    }

    /// Old arguments `lead_start` leaves out and names in `dropped`; every
    /// other old argument after the program is carried.
    pub fn set_lead_start_drops(&self, words: &[&str]) {
        lock(&self.state).lead_start_drops = words.iter().map(|w| (*w).to_owned()).collect();
    }

    /// Warnings `lead_start` gives with every command line from now on.
    pub fn set_lead_start_warnings(&self, lines: &[&str]) {
        lock(&self.state).lead_start_warnings = lines.iter().map(|w| (*w).to_owned()).collect();
    }

    /// `lead_start` fails with `err` until cleared.
    pub fn fail_lead_start(&self, err: Option<HarnessError>) {
        lock(&self.state).lead_start_fault = err;
    }
}
