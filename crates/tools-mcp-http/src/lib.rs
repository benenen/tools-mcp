//! HTTP request execution, layered on `tools-mcp-core` and (optionally) `Tunnel`.

pub mod executor;
pub mod request;

pub use executor::HttpExecutor;
pub use request::{HttpAuth, HttpRequestSpec};
