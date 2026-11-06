//! `teamwire user-key`: make the human user's key when it is missing, and
//! print the file's path, never the key (ADR 0040). A program that sends as
//! `user`, such as the dashboard, reads the key from that path.

use std::process::ExitCode;

use crate::Global;
use crate::cmd::shared::{STORE, fail, home_or_refuse, say};
use crate::wiring::Wiring;

pub(crate) fn run(global: &Global, wiring: &Wiring) -> ExitCode {
    let home = match home_or_refuse("user-key", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    match teamwire_launch::ensure_user_key(wiring.platform.as_ref(), &home) {
        Ok(path) => {
            say(&format!("{}\n", path.display()));
            ExitCode::SUCCESS
        }
        Err(e) => fail("user-key", &e, STORE),
    }
}
