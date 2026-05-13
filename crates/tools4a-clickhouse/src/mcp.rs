//! `clickhouse_exec` MCP tool — params + `McpTool` impl.

use crate::orchestrator::{ClickhouseOrchestrator, ClickhouseRequest};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;
use tools4a_core::config::{Config, ConfigLoader, ConfigMerger, Profile, ServiceType, TomlConfig};
use tools4a_core::{
    Error, ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

fn default_format() -> String {
    "toon".to_string()
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ClickhouseExecParams {
    pub query: String,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,

    /// Include HTML UI resource in the response. Disabled by default to
    /// save tokens (~1700 tokens per call). When enabled, returns an
    /// interactive HTML table alongside the JSON data.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_ui: bool,

    /// Output format for the result. Options: "toon" (default), "json".
    /// TOON format saves 30-60% tokens by using indentation-based format
    /// instead of JSON. Set to "json" for traditional JSON output.
    #[serde(default = "default_format")]
    pub format: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
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

pub struct ClickhouseMcp;

#[async_trait]
impl McpTool for ClickhouseMcp {
    const NAME: &'static str = "clickhouse_exec";
    const DESCRIPTION: &'static str = "Execute a ClickHouse SQL query over HTTP, optionally through an SSH jump host. \
         Reads are allowed by default; writes (INSERT/ALTER/DROP/etc.) require allow_write=true.";
    type Params = ClickhouseExecParams;

    async fn invoke(params: ClickhouseExecParams) -> Result<ExecutionResult> {
        let allow_write = params.allow_write;
        let query = params.query.clone();
        let toml = ConfigLoader::load_default_toml()?;
        let max_timeout_secs = toml.as_ref().and_then(|t| t.defaults.max_timeout_secs);
        let config = params_to_config(&params, toml)?;
        let tunnel = config.tunnel.clone();
        let mut req = ClickhouseRequest::from_config(config, query)?;
        if let Some(ts) = params.timeout_secs {
            req.timeout_secs = Some(ts);
        }
        req.max_timeout_secs = max_timeout_secs;
        req.allow_write = allow_write;
        ClickhouseOrchestrator::execute(req, tunnel).await
    }
}

fn params_to_config(p: &ClickhouseExecParams, toml: Option<TomlConfig>) -> Result<Config> {
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
        service_type: Some(ServiceType::Clickhouse),
        host: p.host.clone(),
        port: p.port,
        user: p.user.clone(),
        password: p.password.clone(),
        database: p.database.clone(),
        db: None,
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
