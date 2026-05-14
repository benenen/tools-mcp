//! rmcp `ServerHandler` glue. Per-service params + dispatch logic live
//! in each leaf crate's `mcp` module via the `tools4a_core::McpTool`
//! trait. This file is thin: each `#[tool]`-decorated method just calls
//! `<Svc>Mcp::invoke(params)` and wraps the result for rmcp.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ResourceContents, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use tools4a_browser::{BrowserExecParams, BrowserMcp};
use tools4a_clickhouse::{ClickhouseExecParams, ClickhouseMcp};
use tools4a_core::{ExecutionResult, McpTool};
use tools4a_docker::{
    DockerExecMcp, DockerExecParams, DockerInspectMcp, DockerInspectParams, DockerLogsMcp,
    DockerLogsParams, DockerPsMcp, DockerPsParams, DockerRestartMcp, DockerRestartParams,
    DockerStatsMcp, DockerStatsParams, DockerTopMcp, DockerTopParams,
};
use tools4a_http::{HttpExecParams, HttpMcp};
use tools4a_mongo::{MongoExecParams, MongoMcp};
use tools4a_mysql::{MysqlExecParams, MysqlMcp};
use tools4a_pgsql::{PgsqlExecParams, PgsqlMcp};
use tools4a_rabbitmq::{
    RabbitmqGetMessagesMcp, RabbitmqGetMessagesParams, RabbitmqListBindingsMcp,
    RabbitmqListBindingsParams, RabbitmqListQueuesMcp, RabbitmqListQueuesParams,
    RabbitmqOverviewMcp, RabbitmqOverviewParams, RabbitmqQueueInfoMcp, RabbitmqQueueInfoParams,
};
use tools4a_redis::{RedisExecParams, RedisMcp};
use tools4a_ssh::{SshExecParams, SshMcp};

use super::ui;

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

/// SQL-flavored variant of `into_call_result` that optionally embeds an
/// MCP App UI resource (`ui://tools4a/<svc>/result`, `text/html`)
/// alongside the JSON text item when `include_ui` is true. UI is
/// disabled by default to save tokens. When UI is enabled, the JSON text
/// is intelligently compressed based on result size. Supports multiple
/// output formats:
/// - "json" (default): Compact JSON with intelligent compression
/// - "toon": Token-Optimized Object Notation (saves 30-60% tokens)
///
/// The UI always contains the full dataset. Clients without MCP Apps
/// support ignore the resource and see only the text. Errors stay
/// single-item text; no UI for failed calls in v1.
fn into_sql_call_result(
    svc: &'static str,
    res: tools4a_core::Result<ExecutionResult>,
    include_ui: bool,
    format: Option<String>,
) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
    match res {
        Ok(result) => {
            let text = match format.as_deref() {
                Some("toon") => {
                    // TOON format: pipe-delimited, saves 30-60% tokens
                    if include_ui {
                        let compressed = result.compress_for_llm();
                        tools4a_core::compressed_to_toon(&compressed)
                    } else {
                        tools4a_core::to_toon(&result)
                    }
                }
                _ => {
                    // Default: Compact JSON
                    if include_ui {
                        let compressed = result.compress_for_llm();
                        serde_json::to_string(&compressed).map_err(|e| {
                            rmcp::ErrorData::internal_error(
                                format!("serialize compressed result failed: {e}"),
                                None,
                            )
                        })?
                    } else {
                        serde_json::to_string_pretty(&result).map_err(|e| {
                            rmcp::ErrorData::internal_error(
                                format!("serialize result failed: {e}"),
                                None,
                            )
                        })?
                    }
                }
            };

            let mut content = vec![Content::text(text)];

            if include_ui {
                // UI always gets the full result
                let html = ui::render_sql(svc, &result);
                let resource = Content::resource(ResourceContents::TextResourceContents {
                    uri: format!("ui://tools4a/{svc}/result"),
                    mime_type: Some("text/html".to_string()),
                    text: html,
                    meta: None,
                });
                content.push(resource);
            }

            Ok(CallToolResult::success(content))
        }
        Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
    }
}

/// HTTP variant of `into_call_result`: returns the same JSON text and
/// optionally an MCP App UI resource (`ui://tools4a/http/response`,
/// `text/html`) with a status badge, headers panel, and content-type-aware
/// body viewer when `include_ui` is true. UI is disabled by default to
/// save tokens.
fn into_http_call_result(
    res: tools4a_core::Result<ExecutionResult>,
    include_ui: bool,
) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
    match res {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result).map_err(|e| {
                rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
            })?;

            let mut content = vec![Content::text(json)];

            if include_ui {
                let html = ui::render_http(&result);
                let resource = Content::resource(ResourceContents::TextResourceContents {
                    uri: "ui://tools4a/http/response".to_string(),
                    mime_type: Some("text/html".to_string()),
                    text: html,
                    meta: None,
                });
                content.push(resource);
            }

            Ok(CallToolResult::success(content))
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
        let include_ui = params.include_ui;
        let format = Some(params.format.clone());
        into_sql_call_result("mysql", MysqlMcp::invoke(params).await, include_ui, format)
    }

    #[tool(
        description = "Execute a PostgreSQL query, optionally through an SSH jump host. Reads are allowed by default; writes require allow_write=true."
    )]
    async fn pgsql_exec(
        &self,
        Parameters(params): Parameters<PgsqlExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        let include_ui = params.include_ui;
        let format = Some(params.format.clone());
        into_sql_call_result("pgsql", PgsqlMcp::invoke(params).await, include_ui, format)
    }

    #[tool(
        description = "Execute a ClickHouse SQL query over HTTP, optionally through an SSH jump host. Reads are allowed by default; writes (INSERT/ALTER/DROP/etc.) require allow_write=true."
    )]
    async fn clickhouse_exec(
        &self,
        Parameters(params): Parameters<ClickhouseExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        let include_ui = params.include_ui;
        let format = Some(params.format.clone());
        into_sql_call_result(
            "clickhouse",
            ClickhouseMcp::invoke(params).await,
            include_ui,
            format,
        )
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
        let include_ui = params.include_ui;
        into_http_call_result(HttpMcp::invoke(params).await, include_ui)
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

    #[tool(
        description = "Run one `agent-browser` CLI subcommand (browser automation via the external agent-browser binary). Returns exit_code, stdout, stderr. Pass the same `session` across calls to share daemon state (cookies, pages). Phase 1: tunnel=ssh is not yet supported - use the inline workaround in the error message if needed."
    )]
    async fn browser_exec(
        &self,
        Parameters(params): Parameters<BrowserExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(BrowserMcp::invoke(params).await)
    }

    #[tool(
        description = "List Docker containers. Read-only. Supports local unix socket, local/remote TCP, and remote unix socket via SSH tunnel (set unix_socket=/var/run/docker.sock + tunnel=ssh)."
    )]
    async fn docker_ps(
        &self,
        Parameters(params): Parameters<DockerPsParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(DockerPsMcp::invoke(params).await)
    }

    #[tool(description = "Inspect a Docker container. Returns the full JSON spec. Read-only.")]
    async fn docker_inspect(
        &self,
        Parameters(params): Parameters<DockerInspectParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(DockerInspectMcp::invoke(params).await)
    }

    #[tool(
        description = "Fetch container logs (one-shot, no follow). Read-only. Default tail is 100 lines."
    )]
    async fn docker_logs(
        &self,
        Parameters(params): Parameters<DockerLogsParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(DockerLogsMcp::invoke(params).await)
    }

    #[tool(
        description = "One-shot container resource stats snapshot (CPU, memory, network, block IO). Read-only."
    )]
    async fn docker_stats(
        &self,
        Parameters(params): Parameters<DockerStatsParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(DockerStatsMcp::invoke(params).await)
    }

    #[tool(
        description = "List processes running inside a container. Useful for finding the JVM PID. Read-only."
    )]
    async fn docker_top(
        &self,
        Parameters(params): Parameters<DockerTopParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(DockerTopMcp::invoke(params).await)
    }

    #[tool(
        description = "Run a command inside a container (Docker exec API). Returns exit_code, stdout, stderr. Requires allow_write=true (write action)."
    )]
    async fn docker_exec(
        &self,
        Parameters(params): Parameters<DockerExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(DockerExecMcp::invoke(params).await)
    }

    #[tool(description = "Restart a Docker container. Requires allow_write=true (write action).")]
    async fn docker_restart(
        &self,
        Parameters(params): Parameters<DockerRestartParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(DockerRestartMcp::invoke(params).await)
    }

    #[tool(
        description = "List RabbitMQ queues with counts and rates. Read-only. Optional vhost + glob filter."
    )]
    async fn rabbitmq_list_queues(
        &self,
        Parameters(params): Parameters<RabbitmqListQueuesParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(RabbitmqListQueuesMcp::invoke(params).await)
    }

    #[tool(description = "Inspect a single RabbitMQ queue (full JSON spec).")]
    async fn rabbitmq_queue_info(
        &self,
        Parameters(params): Parameters<RabbitmqQueueInfoParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(RabbitmqQueueInfoMcp::invoke(params).await)
    }

    #[tool(
        description = "Peek N messages from a RabbitMQ queue without consuming them (uses ackmode=ack_requeue_true)."
    )]
    async fn rabbitmq_get_messages(
        &self,
        Parameters(params): Parameters<RabbitmqGetMessagesParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(RabbitmqGetMessagesMcp::invoke(params).await)
    }

    #[tool(
        description = "List RabbitMQ bindings (source exchange -> destination queue/exchange + routing key). Read-only."
    )]
    async fn rabbitmq_list_bindings(
        &self,
        Parameters(params): Parameters<RabbitmqListBindingsParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(RabbitmqListBindingsMcp::invoke(params).await)
    }

    #[tool(
        description = "RabbitMQ cluster + node overview (versions, totals, message rates). Read-only."
    )]
    async fn rabbitmq_overview(
        &self,
        Parameters(params): Parameters<RabbitmqOverviewParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(RabbitmqOverviewMcp::invoke(params).await)
    }
}

#[tool_handler]
impl ServerHandler for ToolsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "tools4a: unified MySQL / PostgreSQL / ClickHouse / Redis / MongoDB / \
             HTTP / SSH / Browser tools with optional SSH tunneling (browser \
             tunnel is direct-only in Phase 1; SOCKS via SSH lands in Phase 2). \
             Database reads are allowed by default; writes require \
             allow_write=true. Connection params can come from a TOML profile \
             (~/.config/tools4a/config.toml), a YAML file, or be supplied \
             directly in the tool call.",
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

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::RawContent;

    fn sample_sql_result() -> ExecutionResult {
        ExecutionResult::new(
            vec!["id".into(), "name".into()],
            vec![
                vec!["1".into(), "alice".into()],
                vec!["2".into(), "bob".into()],
            ],
            2,
        )
    }

    #[test]
    fn sql_success_yields_text_plus_ui_resource() {
        let call = into_sql_call_result(
            "mysql",
            Ok(sample_sql_result()),
            true,
            Some("json".to_string()),
        )
        .unwrap();
        assert_eq!(call.content.len(), 2);
        assert_eq!(call.is_error, Some(false));

        let text = call.content[0]
            .raw
            .as_text()
            .expect("first content item is text");
        // include_ui=true uses compact JSON via compress_for_llm — parse
        // as a generic Value (not ExecutionResult) since the compressed
        // shape is different from the uncompressed one.
        let parsed: serde_json::Value = serde_json::from_str(&text.text).expect("text is JSON");
        // The compressed result has either rows or a schema+stats section;
        // either way it carries the column names somewhere.
        let serialized = parsed.to_string();
        assert!(serialized.contains("id"));
        assert!(serialized.contains("name"));

        match &call.content[1].raw {
            RawContent::Resource(embedded) => match &embedded.resource {
                ResourceContents::TextResourceContents {
                    uri,
                    mime_type,
                    text,
                    ..
                } => {
                    assert_eq!(uri, "ui://tools4a/mysql/result");
                    assert_eq!(mime_type.as_deref(), Some("text/html"));
                    assert!(text.contains("<!DOCTYPE html>"));
                    assert!(text.contains(">mysql<"));
                }
                _ => panic!("expected TextResourceContents"),
            },
            other => panic!("expected Resource content, got {other:?}"),
        }
    }

    #[test]
    fn sql_uri_varies_with_svc() {
        for svc in ["pgsql", "clickhouse"] {
            let call =
                into_sql_call_result(svc, Ok(sample_sql_result()), true, Some("json".to_string()))
                    .unwrap();
            match &call.content[1].raw {
                RawContent::Resource(embedded) => match &embedded.resource {
                    ResourceContents::TextResourceContents { uri, .. } => {
                        assert_eq!(uri, &format!("ui://tools4a/{svc}/result"));
                    }
                    _ => panic!("expected TextResourceContents"),
                },
                _ => panic!("expected Resource content"),
            }
        }
    }

    fn http_result_ok() -> ExecutionResult {
        ExecutionResult::new(
            vec!["field".into(), "value".into()],
            vec![
                vec!["status_code".into(), "200".into()],
                vec!["status".into(), "200 OK".into()],
                vec!["header.content-type".into(), "application/json".into()],
                vec!["body".into(), r#"{"ok":true}"#.into()],
            ],
            4,
        )
    }

    #[test]
    fn http_success_yields_text_plus_ui_resource() {
        let call = into_http_call_result(Ok(http_result_ok()), true).unwrap();
        assert_eq!(call.content.len(), 2);
        assert_eq!(call.is_error, Some(false));

        match &call.content[1].raw {
            RawContent::Resource(embedded) => match &embedded.resource {
                ResourceContents::TextResourceContents {
                    uri,
                    mime_type,
                    text,
                    ..
                } => {
                    assert_eq!(uri, "ui://tools4a/http/response");
                    assert_eq!(mime_type.as_deref(), Some("text/html"));
                    assert!(text.contains("<!DOCTYPE html>"));
                    assert!(text.contains("status-2xx"));
                }
                _ => panic!("expected TextResourceContents"),
            },
            _ => panic!("expected Resource content"),
        }
    }

    #[test]
    fn sql_error_yields_single_text_item_with_error_flag() {
        let call = into_sql_call_result(
            "mysql",
            Err(tools4a_core::Error::Execution("syntax error".into())),
            false,
            None,
        )
        .unwrap();
        assert_eq!(call.content.len(), 1);
        assert_eq!(call.is_error, Some(true));
        let text = call.content[0]
            .raw
            .as_text()
            .expect("error content is text");
        assert!(text.text.contains("syntax error"));
    }
}
