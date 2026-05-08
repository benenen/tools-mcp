//! HTTP request execution, layered on `tools-mcp-core` and (optionally) `Tunnel`.

pub mod request;

pub use request::{HttpAuth, HttpRequestSpec};
