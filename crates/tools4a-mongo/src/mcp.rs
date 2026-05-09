//! `mongo_exec` MCP tool — params + `McpTool` impl.

use crate::orchestrator::{MongoOrchestrator, MongoRequest};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;
use tools4a_core::config::{Config, ConfigLoader, ConfigMerger, Profile, ServiceType};
use tools4a_core::{
    Error, ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MongoExecParams {
    pub command: String,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,

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
}

pub struct MongoMcp;

#[async_trait]
impl McpTool for MongoMcp {
    const NAME: &'static str = "mongo_exec";
    const DESCRIPTION: &'static str = "Execute a MongoDB command (JSON object passed to runCommand), \
         optionally through an SSH jump host. Reads (find/aggregate \
         without $out/$merge/count/distinct/list*) are allowed by default; \
         writes (insert/update/delete/drop/findAndModify/etc.) require \
         allow_write=true.";
    type Params = MongoExecParams;

    async fn invoke(params: MongoExecParams) -> Result<ExecutionResult> {
        let allow_write = params.allow_write;
        let command = params.command.clone();
        let config = params_to_config(&params)?;
        let tunnel = config.tunnel.clone();
        let mut req = MongoRequest::from_config(config, command)?;
        req.allow_write = allow_write;
        MongoOrchestrator::execute(req, tunnel).await
    }
}

fn params_to_config(p: &MongoExecParams) -> Result<Config> {
    let mut configs: Vec<Config> = Vec::new();

    if let Some(profile_name) = &p.profile {
        let toml_config = ConfigLoader::load_default_toml()?.ok_or_else(|| {
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
        service_type: Some(ServiceType::Mongo),
        host: p.host.clone(),
        port: p.port,
        user: p.user.clone(),
        password: p.password.clone(),
        database: p.database.clone(),
        db: None,
        key_path: None,
        tunnel: tunnel_config,
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
    }
}
