//! rmcp `ServerHandler` glue. Per-service params + dispatch logic live
//! in each leaf crate's `mcp` module via the `tools4a_core::McpTool`
//! trait. This file is thin: each `#[tool]`-decorated method just calls
//! `<Svc>Mcp::invoke(params)` and wraps the result for rmcp.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use tools4a_core::{ExecutionResult, McpTool};
use tools4a_http::{HttpExecParams, HttpMcp};
use tools4a_mongo::{MongoExecParams, MongoMcp};
use tools4a_mysql::{MysqlExecParams, MysqlMcp};
use tools4a_pgsql::{PgsqlExecParams, PgsqlMcp};
use tools4a_redis::{RedisExecParams, RedisMcp};
use tools4a_ssh::{SshExecParams, SshMcp};

#[derive(Debug, Clone)]
pub struct ToolsMcpServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

/// Render an `McpTool::invoke` outcome as an rmcp `CallToolResult`. Same
/// shape for every service, so the per-service `#[tool]` methods stay
/// one-liners.
fn into_call_result(
    res: tools4a_core::Result<ExecutionResult>,
) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
    match res {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result).map_err(|e| {
                rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
            })?;
            Ok(CallToolResult::success(vec![Content::text(json)]))
        }
        Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
    }
}

#[tool_router]
impl ToolsMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Execute a MySQL query, optionally through an SSH jump host. Reads are allowed by default; writes (INSERT/UPDATE/DELETE/DDL) require allow_write=true. Same connection options as the `tools4a mysql` CLI subcommand."
    )]
    async fn mysql_exec(
        &self,
        Parameters(params): Parameters<MysqlExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(MysqlMcp::invoke(params).await)
    }

    #[tool(
        description = "Execute a PostgreSQL query, optionally through an SSH jump host. Reads are allowed by default; writes require allow_write=true."
    )]
    async fn pgsql_exec(
        &self,
        Parameters(params): Parameters<PgsqlExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(PgsqlMcp::invoke(params).await)
    }

    #[tool(
        description = "Execute a Redis command, optionally through an SSH jump host. Same connection options as the `tools4a redis` CLI subcommand."
    )]
    async fn redis_exec(
        &self,
        Parameters(params): Parameters<RedisExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(RedisMcp::invoke(params).await)
    }

    #[tool(
        description = "Execute a MongoDB command (JSON object passed to runCommand), optionally through an SSH jump host. Reads are allowed by default; writes require allow_write=true."
    )]
    async fn mongo_exec(
        &self,
        Parameters(params): Parameters<MongoExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(MongoMcp::invoke(params).await)
    }

    #[tool(
        description = "Send an HTTP/HTTPS request and return status, headers, and body. Optionally route through an SSH jump host."
    )]
    async fn http_exec(
        &self,
        Parameters(params): Parameters<HttpExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(HttpMcp::invoke(params).await)
    }

    #[tool(
        description = "Execute a shell command on a remote SSH server. Returns exit_code, stdout, and stderr. Optionally route through one or more SSH jump hosts; jump credentials and target credentials are independent."
    )]
    async fn ssh_exec(
        &self,
        Parameters(params): Parameters<SshExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(SshMcp::invoke(params).await)
    }
}

#[tool_handler]
impl ServerHandler for ToolsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "tools4a: unified MySQL / PostgreSQL / Redis / MongoDB / HTTP / SSH \
             tools with optional SSH tunneling. Database reads are allowed by \
             default; writes require allow_write=true. Connection params can \
             come from a TOML profile (~/.config/tools4a/config.toml), a YAML \
             file, or be supplied directly in the tool call.",
        )
    }
}

/// Run the MCP server over stdio. Blocks until the client disconnects.
pub async fn serve_stdio() -> crate::Result<()> {
    let server = ToolsMcpServer::new();
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| crate::Error::Connection(format!("MCP server start failed: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| crate::Error::Connection(format!("MCP server error: {e}")))?;
    Ok(())
}
