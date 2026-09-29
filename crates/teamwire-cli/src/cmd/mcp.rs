//! `teamwire mcp`: the stdio MCP server.

use std::process::ExitCode;

use crate::wiring::Wiring;
use crate::{Global, McpArgs};

pub(crate) fn run(args: &McpArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let McpArgs {
        team: _team,
        role: _role,
        token: _token,
        lead: _lead,
    } = args;
    crate::not_built("mcp", global, wiring, false)
}
