//! MySQL connection + executor primitives, layered on `tools-mcp-core`.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::MySQLConnection;
pub use execute::{execute, MysqlParams};
pub use executor::MySQLExecutor;
