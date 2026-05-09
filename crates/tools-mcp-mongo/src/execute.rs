//! Top-level entry: build a Mongo connection over the supplied tunnel,
//! run a single command, return the structured result.

use tools_mcp_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::MongoConnection;
use crate::executor::MongoExecutor;

#[derive(Debug, Clone)]
pub struct MongoParams {
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: String,
}

pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: MongoParams,
    command_str: &str,
) -> Result<ExecutionResult> {
    let mut conn = MongoConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = MongoExecutor::execute(&mut conn, command_str).await;
    let _ = conn.disconnect().await;
    exec_result
}
