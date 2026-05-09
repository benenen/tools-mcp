//! Top-level entry: build a Pgsql connection over the supplied tunnel,
//! run a single query, and return the structured result.

use tools_mcp_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::PgsqlConnection;
use crate::executor::PgsqlExecutor;

#[derive(Debug, Clone)]
pub struct PgsqlParams {
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: PgsqlParams,
    query: &str,
) -> Result<ExecutionResult> {
    let mut conn = PgsqlConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = PgsqlExecutor::execute(&mut conn, query).await;
    let _ = conn.disconnect().await;
    exec_result
}
