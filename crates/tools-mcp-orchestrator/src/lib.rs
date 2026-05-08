//! Orchestrator layer: glues service libs (mysql/redis/http/ssh) together
//! with the `Service` trait, the `Config` / `Profile` / `ConfigLoader` /
//! `ConfigMerger` types for 3-layer merge, and the `DirectTunnel` /
//! `SshTunnel` runtime impls. The bin (cli + mcp) calls into here.

pub mod config;
pub mod tunnel;
