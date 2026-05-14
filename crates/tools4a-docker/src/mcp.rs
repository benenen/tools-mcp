//! Seven `McpTool` impls: one per action. All share a small helper
//! (`build_tunnel_for_docker`) that maps the common tunnel fields into
//! a `TunnelConfig`. Each tool's `Params` is shaped for that action's
//! arguments; the only common bits are the connection fields.

use crate::actions::DockerAction;
use crate::orchestrator::{DockerOrchestrator, DockerRequest};
use async_trait::async_trait;
use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;
use tools4a_core::{
    ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

// -- Shared connection fields ----------------------------------------

/// Connection + tunnel fields, shared across all docker_* MCP tools.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct DockerConnectionFields {
    /// Docker daemon endpoint. Accepts `unix:///path/to/docker.sock`
    /// (default if omitted) or `tcp://host:port`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_host: Option<String>,

    /// When set together with `tunnel=ssh`, forwards this remote unix
    /// socket path through an SSH `direct-streamlocal` channel. Lets
    /// you reach `/var/run/docker.sock` on a host behind SSH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix_socket: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,

    /// Per-call timeout (seconds). Defaults to 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

fn default_docker_host() -> String {
    "unix:///var/run/docker.sock".to_string()
}

fn build_req(
    conn: DockerConnectionFields,
    action: DockerAction,
    allow_write: bool,
) -> Result<(DockerRequest, Option<tools4a_core::TunnelConfig>)> {
    let tunnel = build_tunnel_config(
        conn.tunnel,
        conn.ssh_jump,
        conn.ssh_user,
        conn.ssh_password,
        conn.ssh_key_path,
        conn.ssh_port,
    )?;
    let req = DockerRequest {
        action,
        docker_host: conn.docker_host.unwrap_or_else(default_docker_host),
        unix_socket: conn.unix_socket,
        allow_write,
        timeout_secs: conn.timeout_secs,
    };
    Ok((req, tunnel))
}

// -- docker_ps ------------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DockerPsParams {
    /// Include stopped containers (default false).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    /// Filters per Docker API. Example: `{"name": ["app"], "status": ["running"]}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<HashMap<String, Vec<String>>>,
    #[serde(flatten)]
    pub conn: DockerConnectionFields,
}

pub struct DockerPsMcp;
#[async_trait]
impl McpTool for DockerPsMcp {
    const NAME: &'static str = "docker_ps";
    const DESCRIPTION: &'static str = "List Docker containers. Read-only. Supports local unix socket, local/remote TCP, and \
         remote unix socket via SSH tunnel (set unix_socket=/var/run/docker.sock + tunnel=ssh).";
    type Params = DockerPsParams;

    async fn invoke(p: DockerPsParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            DockerAction::Ps {
                all: p.all,
                limit: p.limit,
                filters: p.filters,
            },
            false,
        )?;
        DockerOrchestrator::execute(req, tunnel).await
    }
}

// -- docker_inspect -------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DockerInspectParams {
    pub container: String,
    #[serde(flatten)]
    pub conn: DockerConnectionFields,
}

pub struct DockerInspectMcp;
#[async_trait]
impl McpTool for DockerInspectMcp {
    const NAME: &'static str = "docker_inspect";
    const DESCRIPTION: &'static str =
        "Inspect a Docker container. Returns the full JSON spec. Read-only.";
    type Params = DockerInspectParams;

    async fn invoke(p: DockerInspectParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            DockerAction::Inspect {
                container: p.container,
            },
            false,
        )?;
        DockerOrchestrator::execute(req, tunnel).await
    }
}

// -- docker_logs ----------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DockerLogsParams {
    pub container: String,
    /// Number of lines from the tail. Default "100"; pass "all" for full log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<String>,
    #[serde(default = "default_true")]
    pub stdout: bool,
    #[serde(default = "default_true")]
    pub stderr: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub timestamps: bool,
    /// UNIX timestamp lower bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i32>,
    #[serde(flatten)]
    pub conn: DockerConnectionFields,
}

fn default_true() -> bool {
    true
}

pub struct DockerLogsMcp;
#[async_trait]
impl McpTool for DockerLogsMcp {
    const NAME: &'static str = "docker_logs";
    const DESCRIPTION: &'static str =
        "Fetch container logs (one-shot, no follow). Read-only. Default tail is 100 lines.";
    type Params = DockerLogsParams;

    async fn invoke(p: DockerLogsParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            DockerAction::Logs {
                container: p.container,
                tail: p.tail,
                stdout: p.stdout,
                stderr: p.stderr,
                timestamps: p.timestamps,
                since: p.since,
            },
            false,
        )?;
        DockerOrchestrator::execute(req, tunnel).await
    }
}

// -- docker_stats ---------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DockerStatsParams {
    pub container: String,
    #[serde(flatten)]
    pub conn: DockerConnectionFields,
}

pub struct DockerStatsMcp;
#[async_trait]
impl McpTool for DockerStatsMcp {
    const NAME: &'static str = "docker_stats";
    const DESCRIPTION: &'static str =
        "One-shot container resource stats snapshot (CPU, memory, network, block IO). Read-only.";
    type Params = DockerStatsParams;

    async fn invoke(p: DockerStatsParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            DockerAction::Stats {
                container: p.container,
            },
            false,
        )?;
        DockerOrchestrator::execute(req, tunnel).await
    }
}

// -- docker_top -----------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DockerTopParams {
    pub container: String,
    /// Args to pass to `ps`. Default `-ef`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ps_args: Option<String>,
    #[serde(flatten)]
    pub conn: DockerConnectionFields,
}

pub struct DockerTopMcp;
#[async_trait]
impl McpTool for DockerTopMcp {
    const NAME: &'static str = "docker_top";
    const DESCRIPTION: &'static str =
        "List processes running inside a container. Useful for finding the JVM PID. Read-only.";
    type Params = DockerTopParams;

    async fn invoke(p: DockerTopParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            DockerAction::Top {
                container: p.container,
                ps_args: p.ps_args,
            },
            false,
        )?;
        DockerOrchestrator::execute(req, tunnel).await
    }
}

// -- docker_exec (run command inside container) ---------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DockerExecParams {
    pub container: String,
    /// Command + args, e.g. `["sh","-c","jstack 1"]`.
    pub cmd: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Env vars in `KEY=value` form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub privileged: bool,
    /// Required to be true (write action).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,
    #[serde(flatten)]
    pub conn: DockerConnectionFields,
}

pub struct DockerExecMcp;
#[async_trait]
impl McpTool for DockerExecMcp {
    const NAME: &'static str = "docker_exec";
    const DESCRIPTION: &'static str = "Run a command inside a container (Docker exec API). Returns exit_code, stdout, stderr. \
         Requires allow_write=true (write action).";
    type Params = DockerExecParams;

    async fn invoke(p: DockerExecParams) -> Result<ExecutionResult> {
        let allow_write = p.allow_write;
        let (req, tunnel) = build_req(
            p.conn,
            DockerAction::Run {
                container: p.container,
                cmd: p.cmd,
                user: p.user,
                working_dir: p.working_dir,
                env: p.env,
                privileged: p.privileged,
            },
            allow_write,
        )?;
        DockerOrchestrator::execute(req, tunnel).await
    }
}

// -- docker_restart -------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DockerRestartParams {
    pub container: String,
    /// Seconds to wait before killing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,
    #[serde(flatten)]
    pub conn: DockerConnectionFields,
}

pub struct DockerRestartMcp;
#[async_trait]
impl McpTool for DockerRestartMcp {
    const NAME: &'static str = "docker_restart";
    const DESCRIPTION: &'static str =
        "Restart a Docker container. Requires allow_write=true (write action).";
    type Params = DockerRestartParams;

    async fn invoke(p: DockerRestartParams) -> Result<ExecutionResult> {
        let allow_write = p.allow_write;
        let (req, tunnel) = build_req(
            p.conn,
            DockerAction::Restart {
                container: p.container,
                timeout_secs: p.timeout_secs,
            },
            allow_write,
        )?;
        DockerOrchestrator::execute(req, tunnel).await
    }
}
