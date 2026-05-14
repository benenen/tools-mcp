//! Docker leaf crate: typed Docker Engine API access via bollard, with
//! three connection modes — local Unix socket, local/remote TCP, and
//! remote Unix socket via `tools4a_core::StreamLocalTunnel`. Seven
//! MCP tools share one orchestrator and one action dispatcher.
//!
//! See `docs/superpowers/plans/2026-05-14-tools-mcp-phase15-docker.md`.

pub mod actions;
pub mod connection;
pub mod mcp;
pub mod orchestrator;
pub mod run;

pub use actions::DockerAction;
pub use connection::{ConnectTarget, connect_docker};
pub use mcp::{
    DockerExecMcp, DockerExecParams, DockerInspectMcp, DockerInspectParams, DockerLogsMcp,
    DockerLogsParams, DockerPsMcp, DockerPsParams, DockerRestartMcp, DockerRestartParams,
    DockerStatsMcp, DockerStatsParams, DockerTopMcp, DockerTopParams,
};
pub use orchestrator::{DockerOrchestrator, DockerRequest};
pub use run::run;
