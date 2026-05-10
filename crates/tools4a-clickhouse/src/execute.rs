//! Top-level entry: build a Clickhouse HTTP client over the supplied
//! tunnel, run a single query, and return the structured result.

use tools4a_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::ClickhouseConnection;
use crate::executor::ClickhouseExecutor;

#[derive(Debug, Clone)]
pub struct ClickhouseParams {
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

/// Execute a single Clickhouse query through `tunnel`. When `read_only`
/// is true, the HTTP client is configured with `readonly=1` so the
/// server rejects writes — belt-and-suspenders alongside the
/// orchestrator's `is_readonly_sql` gate. Always tears down the
/// tunnel before returning.
pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: ClickhouseParams,
    query: &str,
    read_only: bool,
) -> Result<ExecutionResult> {
    let mut conn = ClickhouseConnection::new(
        tunnel,
        params.user,
        params.password,
        params.database,
        read_only,
    );
    let exec_result = run(&mut conn, query).await;
    let _ = conn.disconnect().await;
    exec_result
}

async fn run(conn: &mut ClickhouseConnection, query: &str) -> Result<ExecutionResult> {
    conn.connect().await?;
    ClickhouseExecutor::execute(conn, query).await
}
