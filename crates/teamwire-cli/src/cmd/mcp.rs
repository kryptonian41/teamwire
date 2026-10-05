//! `teamwire mcp`: the stdio MCP server.

use std::process::ExitCode;
use std::sync::Arc;

use teamwire_core::{Config, Role, TeamName};
use teamwire_mcp::{McpCaller, ServerEnv, serve_stdio};

use crate::cmd::send::{REFUSED, fail, home_or_refuse};
use crate::wiring::{Wiring, WiringEnv, wire};
use crate::{Global, McpArgs, USAGE};

pub(crate) fn run(args: &McpArgs, global: &Global, _wiring: &Wiring) -> ExitCode {
    let McpArgs {
        team,
        role,
        token,
        lead,
    } = args;
    let caller = match (team, role, token, lead) {
        (_, _, _, true) => McpCaller::Lead,
        (Some(team), Some(role), Some(token), false) => {
            match (TeamName::parse(team), Role::parse(role)) {
                (Ok(team), Ok(role)) => McpCaller::Member {
                    team,
                    role,
                    token: token.clone(),
                },
                (Err(e), _) => return fail("mcp", &e.to_string(), USAGE),
                (_, Err(e)) => return fail("mcp", &e.to_string(), USAGE),
            }
        }
        _ => {
            return fail(
                "mcp",
                "name a member with --team, --role and --token, or serve the lead with --lead",
                USAGE,
            );
        }
    };
    // The server keeps its parts on tasks of its own, so it needs parts it
    // owns rather than the borrowed ones of this command.
    let owned = match wire(&WiringEnv::from_process()) {
        Ok(owned) => owned,
        Err(e) => return fail("mcp", &e.to_string(), REFUSED),
    };
    let home = match home_or_refuse("mcp", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let env = ServerEnv {
        home,
        registry: Arc::new(owned.registry),
        platform: owned.platform,
        clock: Arc::new(owned.clock),
        config: Config::default(),
    };
    match serve_stdio(env, caller) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail("mcp", &e.to_string(), REFUSED),
    }
}
