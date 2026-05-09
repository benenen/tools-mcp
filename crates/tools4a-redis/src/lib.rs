//! Redis stack: connection + executor primitives, the
//! `RedisOrchestrator` `Service` impl, and the `RedisMcp` `McpTool` impl.

pub mod connection;
pub mod execute;
pub mod executor;
pub mod mcp;
pub mod orchestrator;

pub use connection::RedisConnection;
pub use execute::{RedisParams, execute};
pub use executor::RedisExecutor;
pub use mcp::{RedisExecParams, RedisMcp};
pub use orchestrator::{RedisOrchestrator, RedisRequest};
