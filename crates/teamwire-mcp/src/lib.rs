//! The stdio MCP server (`rmcp`): the eight tools of `docs/api/mcp.md`, a
//! thin front for `teamwire-msg`, `teamwire-report` and `teamwire-watch`,
//! so every rule and refusal is the CLI's.
//!
//! Never: offer launch, watch, verify, close or removal; hold state between
//! calls; accept a member call without its token or a lead call without the
//! recorded lead session.
//!
//! Invariants: the store is opened for each call and closed after it; a
//! refused call stores nothing.

use std::fmt;
use std::path::Path;

use teamwire_core::{Clock, Config, Platform, Registry, Role, TeamName};
use thiserror::Error;

/// Who the server speaks for: one member, proven by its token on every
/// call, or the lead, proven by its harness session.
#[derive(Clone, PartialEq, Eq)]
pub enum McpCaller {
    Member {
        team: TeamName,
        role: Role,
        token: String,
    },
    Lead,
}

impl fmt::Debug for McpCaller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Member { team, role, .. } => f
                .debug_struct("Member")
                .field("team", team)
                .field("role", role)
                .field("token", &"<redacted>")
                .finish(),
            Self::Lead => f.write_str("Lead"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum McpError {
    #[error("the stdio transport failed: {0}")]
    Transport(String),
    #[error("internal: {0}")]
    Internal(String),
}

/// What the server needs to build a `Ctx` for each call.
#[derive(Clone, Copy)]
pub struct ServerEnv<'a> {
    pub home: &'a Path,
    pub registry: &'a Registry,
    pub platform: &'a dyn Platform,
    pub clock: &'a dyn Clock,
    pub config: &'a Config,
}

impl fmt::Debug for ServerEnv<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerEnv")
            .field("home", &self.home)
            .field("registry", &self.registry)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Serves MCP on stdin and stdout until the client closes the stream.
pub fn serve_stdio(_env: ServerEnv<'_>, _caller: McpCaller) -> Result<(), McpError> {
    Err(McpError::Internal(
        "teamwire-mcp is not built yet".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_the_token() {
        let caller = McpCaller::Member {
            team: TeamName::parse("tw").unwrap(),
            role: Role::parse("api").unwrap(),
            token: "tok-secret-2".to_owned(),
        };
        assert!(!format!("{caller:?}").contains("tok-secret-2"));
    }
}
