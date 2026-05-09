//! PostgreSQL stack: connection + executor primitives, the
//! `PgsqlOrchestrator` `Service` impl, and the `PgsqlMcp` `McpTool` impl.

pub mod connection;
pub mod execute;
pub mod executor;
pub mod mcp;
pub mod orchestrator;

pub use connection::PgsqlConnection;
pub use execute::{PgsqlParams, execute};
pub use executor::PgsqlExecutor;
pub use mcp::{PgsqlExecParams, PgsqlMcp};
pub use orchestrator::{PgsqlOrchestrator, PgsqlRequest};
