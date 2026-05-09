//! Orchestrator layer: glues service libs (mysql/redis/http/ssh) together
//! with the `Service` trait, the `Config` / `Profile` / `ConfigLoader` /
//! `ConfigMerger` types for 3-layer merge, and the `DirectTunnel` /
//! `SshTunnel` runtime impls. The bin (cli + mcp) calls into here.

pub mod config;
pub mod http;
pub mod mongo;
pub mod mysql;
pub mod pgsql;
pub mod redis;
pub mod ssh;
pub mod tunnel;

pub use http::HttpOrchestrator;
pub use mongo::{MongoOrchestrator, MongoRequest};
pub use mysql::{MysqlOrchestrator, MysqlRequest};
pub use pgsql::{PgsqlOrchestrator, PgsqlRequest};
pub use redis::{RedisOrchestrator, RedisRequest};
pub use ssh::SshDirectOrchestrator;

// Re-exports for the bin so it doesn't need direct service-lib deps.
pub use tools_mcp_http::{HttpAuth, HttpRequestSpec};
pub use tools_mcp_ssh::SshExecRequest;
