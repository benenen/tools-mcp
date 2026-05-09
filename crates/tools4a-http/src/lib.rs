//! HTTP request execution, layered on `tools4a-core` and (optionally) `Tunnel`.

pub mod execute;
pub mod executor;
pub mod request;

pub use execute::execute;
pub use executor::HttpExecutor;
pub use request::{HttpAuth, HttpRequestSpec};
