//! Redis connection + executor primitives, layered on `tools4a-core`.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::RedisConnection;
pub use execute::{RedisParams, execute};
pub use executor::RedisExecutor;
