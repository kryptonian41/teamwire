//! The `rmcp` handler: lists the ten tools and runs each call on a
//! blocking thread, because the service functions and the store are
//! synchronous.

use std::sync::Arc;

use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
    Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};

use crate::failure::ToolFailure;
use crate::tools::{self, Reply, TOOLS};
use crate::{McpCaller, ServerEnv};

/// The MCP server for one caller. It holds no state between calls beyond
/// who it speaks for and where the state directory is.
#[derive(Debug, Clone)]
pub struct McpServer {
    env: Arc<ServerEnv>,
    caller: Arc<McpCaller>,
}

impl McpServer {
    pub fn new(env: ServerEnv, caller: McpCaller) -> Self {
        Self {
            env: Arc::new(env),
            caller: Arc::new(caller),
        }
    }
}

fn success(reply: Reply) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(reply.summary)]);
    result.structured_content = Some(reply.value);
    result
}

fn refusal(failure: &ToolFailure) -> CallToolResult {
    let mut result = CallToolResult::error(vec![ContentBlock::text(format!(
        "{}: {}",
        failure.code, failure.message
    ))]);
    result.structured_content = Some(failure.to_json());
    result
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("teamwire", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let listed = TOOLS
            .iter()
            .filter_map(|(name, description)| {
                let schema = crate::inputs::input_schema(name)?;
                Some(Tool::new(*name, *description, Arc::new(schema)))
            })
            .collect();
        // A client on protocol 2026-07-28 (Claude Code is one) rejects a
        // listing that carries `resultType` but no cache fields, and `rmcp`
        // adds `resultType` whenever the request carries the 2026 envelope.
        // So the listing always says: do not cache, and only for this user.
        Ok(ListToolsResult {
            tools: listed,
            ..ListToolsResult::default()
        }
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let env = Arc::clone(&self.env);
        let caller = Arc::clone(&self.caller);
        let name = request.name.to_string();
        let args = request.arguments.unwrap_or_default();
        let outcome =
            tokio::task::spawn_blocking(move || tools::call(&env, &caller, &name, args)).await;
        let result = match outcome {
            Ok(Ok(reply)) => success(reply),
            Ok(Err(failure)) => refusal(&failure),
            Err(e) => refusal(&ToolFailure::new(
                "internal",
                format!("the call did not finish: {e}"),
            )),
        };
        Ok(result.into())
    }
}
