//! `ssh_exec` MCP tool — params + `McpTool` impl. Like HTTP, no
//! Profile/YAML support: params land directly in `SshExecRequest` +
//! optional jump-host config.

use crate::orchestrator::SshDirectOrchestrator;
use crate::request::SshExecRequest;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tools4a_core::config::ConfigLoader;
use tools4a_core::{
    Error, ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SshExecParams {
    pub command: String,
    pub host: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    pub user: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,

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

    /// Per-call execution timeout in seconds. Capped by the operator's
    /// `TOOLS4A_MAX_TIMEOUT_SECS` env var or TOML `[defaults]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

pub struct SshMcp;

#[async_trait]
impl McpTool for SshMcp {
    const NAME: &'static str = "ssh_exec";
    const DESCRIPTION: &'static str = "Execute a shell command on a remote SSH server. Returns exit_code, \
         stdout, and stderr. Optionally route through one or more SSH jump \
         hosts; jump credentials and target credentials are independent.";
    type Params = SshExecParams;

    async fn invoke(params: SshExecParams) -> Result<ExecutionResult> {
        let max_timeout_secs =
            ConfigLoader::load_default_toml()?.and_then(|t| t.defaults.max_timeout_secs);
        let (mut req, tunnel) = params_to_request_and_tunnel(params)?;
        req.max_timeout_secs = max_timeout_secs;
        SshDirectOrchestrator::execute(req, tunnel).await
    }
}

fn params_to_request_and_tunnel(
    p: SshExecParams,
) -> Result<(SshExecRequest, Option<tools4a_core::TunnelConfig>)> {
    if p.password.is_some() && p.key_path.is_some() {
        return Err(Error::Config(
            "password and key_path are mutually exclusive".to_string(),
        ));
    }

    let req = SshExecRequest {
        host: p.host,
        port: p.port.unwrap_or(22),
        user: p.user,
        password: p.password,
        key_path: p.key_path.map(std::path::PathBuf::from),
        command: p.command,
        timeout_secs: p.timeout_secs,
        max_timeout_secs: None,
    };

    let tunnel_config = build_tunnel_config(
        p.tunnel,
        p.ssh_jump,
        p.ssh_user,
        p.ssh_password,
        p.ssh_key_path,
        p.ssh_port,
    )?;

    Ok((req, tunnel_config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_request() {
        let p = SshExecParams {
            command: "uptime".into(),
            host: "server.com".into(),
            port: None,
            user: "admin".into(),
            password: Some("pwd".into()),
            key_path: None,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
            timeout_secs: None,
        };
        let (req, tunnel) = params_to_request_and_tunnel(p).unwrap();
        assert_eq!(req.command, "uptime");
        assert_eq!(req.port, 22);
        assert!(tunnel.is_none());
    }

    #[test]
    fn password_and_key_mutex() {
        let p = SshExecParams {
            command: "ls".into(),
            host: "h".into(),
            port: None,
            user: "u".into(),
            password: Some("pwd".into()),
            key_path: Some("/k".into()),
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
            timeout_secs: None,
        };
        let err = params_to_request_and_tunnel(p).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("mutually exclusive")));
    }
}
