//! The stdio MCP server (`rmcp`): the nine tools of `docs/api/mcp.md`, a
//! thin front for `teamwire-msg`, `teamwire-report` and `teamwire-watch`,
//! so every rule and refusal is the CLI's.
//!
//! Never: offer launch, watch, verify, close or removal; hold state between
//! calls; accept a member call without its token or a lead call without the
//! recorded lead session.
//!
//! Invariants: the store is opened for each call and closed after it; a
//! refused call stores nothing.

mod failure;
mod inputs;
mod reads;
mod server;
mod tools;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::ServiceExt;
use teamwire_core::{Clock, Config, Platform, Registry, Role, TeamName};
use thiserror::Error;

pub use server::McpServer;

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

/// What the server needs to build a `Ctx` for each call. It owns its parts
/// because `rmcp` runs each call on its own task.
#[derive(Clone)]
pub struct ServerEnv {
    pub home: PathBuf,
    pub registry: Arc<Registry>,
    pub platform: Arc<dyn Platform>,
    pub clock: Arc<dyn Clock>,
    pub config: Config,
}

impl fmt::Debug for ServerEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerEnv")
            .field("home", &self.home)
            .field("registry", &self.registry)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Serves MCP on stdin and stdout until the client closes the stream.
/// Stdout carries the protocol, so nothing else may print to it.
pub fn serve_stdio(env: ServerEnv, caller: McpCaller) -> Result<(), McpError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| McpError::Internal(format!("could not start the runtime: {e}")))?;
    runtime.block_on(async {
        let running = McpServer::new(env, caller)
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;
        running
            .waiting()
            .await
            .map(|_| ())
            .map_err(|e| McpError::Transport(e.to_string()))
    })
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
