//! Redis connection + executor primitives, layered on `tools-mcp-core`.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::RedisConnection;
pub use execute::{execute, RedisParams};
pub use executor::RedisExecutor;
