//! Top-level entry: build a MySQL connection over the supplied tunnel,
//! run a single query, and return the structured result.

use tools4a_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::MySQLConnection;
use crate::executor::MySQLExecutor;

/// Required MySQL connection parameters (post-merge in the caller).
#[derive(Debug, Clone)]
pub struct MysqlParams {
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

/// Execute a single MySQL query through `tunnel`. Always tears down the
/// connection (and via Drop, the tunnel) before returning.
pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: MysqlParams,
    query: &str,
) -> Result<ExecutionResult> {
    let mut conn = MySQLConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = MySQLExecutor::execute(&mut conn, query).await;
    let _ = conn.disconnect().await;
    exec_result
}
