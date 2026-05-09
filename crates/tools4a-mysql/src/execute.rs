//! Top-level entry: build a MySQL connection over the supplied tunnel,
//! run a single query, and return the structured result.

use mysql_async::prelude::Queryable;
use tools4a_core::{Connection, Error, ExecutionResult, Result, Tunnel};

use crate::connection::MySQLConnection;
use crate::executor::MySQLExecutor;

/// Required MySQL connection parameters (post-merge in the caller).
#[derive(Debug, Clone)]
pub struct MysqlParams {
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

/// Execute a single MySQL query through `tunnel`. When `read_only` is
/// true, the session is set to `TRANSACTION READ ONLY` before the query
/// runs — MySQL rejects writes with error 1792 in that mode.
/// Always tears down the connection (and via Drop, the tunnel) before
/// returning.
pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: MysqlParams,
    query: &str,
    read_only: bool,
) -> Result<ExecutionResult> {
    let mut conn = MySQLConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = run(&mut conn, query, read_only).await;
    let _ = conn.disconnect().await;
    exec_result
}

async fn run(conn: &mut MySQLConnection, query: &str, read_only: bool) -> Result<ExecutionResult> {
    if read_only {
        let mc = conn.get_conn().await?;
        mc.query_drop("SET SESSION TRANSACTION READ ONLY")
            .await
            .map_err(|e: mysql_async::Error| Error::Service(format!("MySQL set read_only: {e}")))?;
    }
    MySQLExecutor::execute(conn, query).await
}
