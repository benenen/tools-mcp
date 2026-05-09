//! HTTP stack: request execution primitives, the `HttpOrchestrator`
//! `Service` impl, and the `HttpMcp` `McpTool` impl.

pub mod execute;
pub mod executor;
pub mod mcp;
pub mod orchestrator;
pub mod request;

pub use execute::execute;
pub use executor::HttpExecutor;
pub use mcp::{HttpExecParams, HttpMcp};
pub use orchestrator::HttpOrchestrator;
pub use request::{HttpAuth, HttpRequestSpec};
