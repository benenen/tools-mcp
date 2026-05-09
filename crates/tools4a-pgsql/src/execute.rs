//! Top-level entry: build a Pgsql connection over the supplied tunnel,
//! run a single query, and return the structured result.

use tools4a_core::{Connection, Error, ExecutionResult, Result, Tunnel};

use crate::connection::PgsqlConnection;
use crate::executor::PgsqlExecutor;

#[derive(Debug, Clone)]
pub struct PgsqlParams {
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

/// Execute a single Pgsql query through `tunnel`. When `read_only` is
/// true, the session's `default_transaction_read_only` GUC is enabled
/// before the query runs — Postgres rejects writes with `25006` in that
/// mode. Always tears down the connection before returning.
pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: PgsqlParams,
    query: &str,
    read_only: bool,
) -> Result<ExecutionResult> {
    let mut conn = PgsqlConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = run(&mut conn, query, read_only).await;
    let _ = conn.disconnect().await;
    exec_result
}

async fn run(conn: &mut PgsqlConnection, query: &str, read_only: bool) -> Result<ExecutionResult> {
    conn.connect().await?;
    if read_only {
        let client = conn.client()?;
        client
            .simple_query("SET default_transaction_read_only = on")
            .await
            .map_err(|e| Error::Service(format!("Pgsql set read_only: {e}")))?;
    }
    PgsqlExecutor::execute(conn, query).await
}
