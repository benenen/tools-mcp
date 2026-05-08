//! Top-level entry: build a Redis connection over the supplied tunnel,
//! run a single command, and return the structured result.

use tools_mcp_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::RedisConnection;
use crate::executor::RedisExecutor;

#[derive(Debug, Clone)]
pub struct RedisParams {
    pub password: Option<String>,
    /// Redis database number (0..15 in default Redis configs).
    pub db: u32,
}

/// Execute a single Redis command through `tunnel`. Always tears down the
/// connection (and via Drop, the tunnel) before returning.
pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: RedisParams,
    command_str: &str,
) -> Result<ExecutionResult> {
    let mut conn = RedisConnection::new(tunnel, params.password, params.db);
    let exec_result = RedisExecutor::run(&mut conn, command_str).await;
    let _ = conn.disconnect().await;
    exec_result
}
