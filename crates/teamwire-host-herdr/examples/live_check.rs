//! Read-only check of the herdr adapter against the real herdr, run by hand:
//!
//! ```text
//! cargo run -p teamwire-host-herdr --example live_check --locked
//! ```
//!
//! It calls only `agent list` and `agent get` (on the pane named by
//! `HERDR_PANE_ID`, when set) and prints what the adapter parsed. It starts,
//! types into, renames and closes nothing. No test runs it.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a hand-run example reports to the person who ran it"
)]

use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::{Host, Place};
use teamwire_host_herdr::Herdr;

const T: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    let herdr = Herdr::new(teamwire_platform::native());
    let mut failed = false;

    match herdr.live_agent_names(T) {
        Ok(names) => println!("live_agent_names: {} named agents: {names:?}", names.len()),
        Err(e) => {
            eprintln!("live_agent_names failed: {e}");
            failed = true;
        }
    }
    match herdr.list(T) {
        Ok(places) => println!("list: {} places", places.len()),
        Err(e) => {
            eprintln!("list failed: {e}");
            failed = true;
        }
    }

    let Some(var) = herdr.place_env_var() else {
        return ExitCode::FAILURE;
    };
    match std::env::var(var) {
        Ok(id) => {
            let place = Place {
                host: herdr.id(),
                id,
            };
            println!("own place: {}", place.id);
            println!("state: {:?}", herdr.state(&place, T));
            println!("occupant: {:?}", herdr.occupant(&place, T));
        }
        Err(_) => println!("{var} is not set: skipped the calls on this pane"),
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
