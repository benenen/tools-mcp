use crate::mcp::tools::{MysqlExecParams, mysql_exec};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};

#[derive(Debug, Clone)]
pub struct ToolsMcpServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ToolsMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Execute a MySQL query, optionally through an SSH tunnel.
    #[tool(
        description = "Execute a MySQL query, optionally through an SSH jump host. Same connection options as the `tools-mcp mysql` CLI subcommand."
    )]
    async fn mysql_exec(
        &self,
        Parameters(params): Parameters<MysqlExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        match mysql_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    /// Execute a PostgreSQL query, optionally through an SSH tunnel.
    #[tool(
        description = "Execute a PostgreSQL query, optionally through an SSH jump host. Same connection options as the `tools-mcp pgsql` CLI subcommand."
    )]
    async fn pgsql_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::PgsqlExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::pgsql_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json),
                ]))
            }
            Err(e) => Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(e.to_string()),
            ])),
        }
    }

    /// Execute a Redis command, optionally through an SSH tunnel.
    #[tool(
        description = "Execute a Redis command, optionally through an SSH jump host. Same connection options as the `tools-mcp redis` CLI subcommand."
    )]
    async fn redis_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::RedisExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::redis_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json),
                ]))
            }
            Err(e) => Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(e.to_string()),
            ])),
        }
    }

    /// Execute a MongoDB command, optionally through an SSH tunnel.
    #[tool(
        description = "Execute a MongoDB command (JSON object passed to runCommand), optionally through an SSH jump host. Same connection options as the `tools-mcp mongo` CLI subcommand."
    )]
    async fn mongo_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::MongoExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::mongo_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json),
                ]))
            }
            Err(e) => Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(e.to_string()),
            ])),
        }
    }

    /// Execute an HTTP request, optionally through an SSH tunnel.
    #[tool(
        description = "Send an HTTP/HTTPS request and return status, headers, and body. Optionally route through an SSH jump host. Same options as the `tools-mcp http` CLI subcommand."
    )]
    async fn http_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::HttpExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::http_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json),
                ]))
            }
            Err(e) => Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(e.to_string()),
            ])),
        }
    }

    /// Run a shell command on an SSH target, optionally through SSH jumps.
    #[tool(
        description = "Execute a shell command on a remote SSH server. Returns exit_code, stdout, and stderr. Optionally route through one or more SSH jump hosts; jump credentials and target credentials are independent."
    )]
    async fn ssh_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::SshExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::ssh_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json),
                ]))
            }
            Err(e) => Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(e.to_string()),
            ])),
        }
    }
}

#[tool_handler]
impl ServerHandler for ToolsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "tools-mcp: MySQL query execution with optional SSH tunneling. \
                 Use the mysql_exec tool. Connection params can come from a TOML \
                 profile (~/.config/tools-mcp/config.toml), a YAML file, or be \
                 supplied directly in the tool call.",
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
