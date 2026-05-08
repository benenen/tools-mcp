//! Redis connection + executor primitives, layered on `tools-mcp-core`.

pub mod connection;

pub use connection::RedisConnection;
