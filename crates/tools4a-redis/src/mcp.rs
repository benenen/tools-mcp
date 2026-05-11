//! `redis_exec` MCP tool — params + `McpTool` impl.

use crate::orchestrator::{RedisOrchestrator, RedisRequest};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;
use tools4a_core::config::{Config, ConfigLoader, ConfigMerger, Profile, ServiceType, TomlConfig};
use tools4a_core::{
    Error, ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RedisExecParams {
    pub command: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,

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

pub struct RedisMcp;

#[async_trait]
impl McpTool for RedisMcp {
    const NAME: &'static str = "redis_exec";
    const DESCRIPTION: &'static str = "Execute a Redis command, optionally through an SSH jump host. \
         Same connection options as the `tools4a redis` CLI subcommand.";
    type Params = RedisExecParams;

    async fn invoke(params: RedisExecParams) -> Result<ExecutionResult> {
        let command = params.command.clone();
        let toml = ConfigLoader::load_default_toml()?;
        let max_timeout_secs = toml.as_ref().and_then(|t| t.defaults.max_timeout_secs);
        let config = params_to_config(&params, toml)?;
        let tunnel = config.tunnel.clone();
        let mut req = RedisRequest::from_config(config, command)?;
        if let Some(ts) = params.timeout_secs {
            req.timeout_secs = Some(ts);
        }
        req.max_timeout_secs = max_timeout_secs;
        RedisOrchestrator::execute(req, tunnel).await
    }
}

fn params_to_config(p: &RedisExecParams, toml: Option<TomlConfig>) -> Result<Config> {
    let mut configs: Vec<Config> = Vec::new();

    if let Some(profile_name) = &p.profile {
        let toml_config = toml.ok_or_else(|| {
            Error::Config(format!(
                "profile '{profile_name}' requested but no ~/.config/tools4a/config.toml found"
            ))
        })?;
        let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
            Error::Config(format!("profile '{profile_name}' not found in config.toml"))
        })?;
        configs.push(profile_to_config(profile_cfg));
    }

    if let Some(path) = p.config.as_deref() {
        configs.push(ConfigLoader::load_yaml_file(path)?);
    }

    let tunnel_config = build_tunnel_config(
        p.tunnel.clone(),
        p.ssh_jump.clone(),
        p.ssh_user.clone(),
        p.ssh_password.clone(),
        p.ssh_key_path.clone(),
        p.ssh_port,
    )?;

    configs.push(Config {
        service_type: Some(ServiceType::Redis),
        host: p.host.clone(),
        port: p.port,
        user: None,
        password: p.password.clone(),
        database: None,
        db: p.db,
        key_path: None,
        tunnel: tunnel_config,
        timeout_secs: p.timeout_secs,
    });

    Ok(ConfigMerger::merge_multiple(configs))
}

fn profile_to_config(profile: &Profile) -> Config {
    Config {
        service_type: Some(profile.service_type.clone()),
        host: profile.host.clone(),
        port: profile.port,
        user: profile.user.clone(),
        password: profile.password.clone(),
        database: profile.database.clone(),
        db: profile.db,
        key_path: profile.key_path.clone(),
        tunnel: profile.tunnel.clone(),
        timeout_secs: profile.timeout_secs,
    }
}
