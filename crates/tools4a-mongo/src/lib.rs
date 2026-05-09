//! MongoDB stack: connection + executor primitives, the
//! `MongoOrchestrator` `Service` impl, and the `MongoMcp` `McpTool` impl.
//!
//! Commands are JSON documents passed to `Database::run_command`. The
//! returned BSON Document is serialized back to JSON and presented as a
//! single `result` row.

pub mod connection;
pub mod execute;
pub mod executor;
pub mod mcp;
pub mod orchestrator;

pub use connection::MongoConnection;
pub use execute::{MongoParams, execute};
pub use executor::MongoExecutor;
pub use mcp::{MongoExecParams, MongoMcp};
pub use orchestrator::{MongoOrchestrator, MongoRequest};
