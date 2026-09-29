//! `teamwire send`: send one message.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, SendArgs};

pub(crate) fn run(args: &SendArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let SendArgs {
        team: _team,
        to: _to,
        kind: _kind,
        text: _text,
        file: _file,
        commit: _commit,
        re: _re,
        key: _key,
        from: _from,
    } = args;
    crate::not_built("send", global, wiring, false)
}
