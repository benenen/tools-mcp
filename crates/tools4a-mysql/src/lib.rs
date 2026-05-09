//! MySQL stack: connection + executor primitives, the `MysqlOrchestrator`
//! `Service` impl, and the `MysqlMcp` `McpTool` impl. The bin's CLI/MCP
//! layers go through `MysqlOrchestrator::execute` (CLI) or
//! `MysqlMcp::invoke` (MCP) — no per-service plumbing in the bin.

pub mod connection;
pub mod execute;
pub mod executor;
pub mod mcp;
pub mod orchestrator;

pub use connection::MySQLConnection;
pub use execute::{MysqlParams, execute};
pub use executor::MySQLExecutor;
pub use mcp::{MysqlExecParams, MysqlMcp};
pub use orchestrator::{MysqlOrchestrator, MysqlRequest};
