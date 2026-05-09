use crate::output::ExecutionResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tools4a_core::{Error, Result, Service, TunnelConfig};
use tools4a_orchestrator::config::{Config, ConfigLoader, ConfigMerger, Profile, ServiceType};
use tools4a_orchestrator::{
    HttpAuth, HttpOrchestrator, HttpRequestSpec, MongoOrchestrator, MongoRequest,
    MysqlOrchestrator, MysqlRequest, PgsqlOrchestrator, PgsqlRequest, RedisOrchestrator,
    RedisRequest, SshDirectOrchestrator, SshExecRequest,
};

/// JSON parameters for the `mysql_exec` MCP tool. Mirrors the CLI's
/// `mysql` subcommand args plus the global tunnel/config flags, so an
/// AI assistant can invoke any MySQL query the CLI can run.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MysqlExecParams {
    /// SQL query to execute.
    pub query: String,

    /// Allow write operations (INSERT/UPDATE/DELETE/DDL). Default false.
    /// When false, the orchestrator rejects non-SELECT queries AND the
    /// session runs in TRANSACTION READ ONLY.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,

    /// MySQL host (overrides profile / yaml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// MySQL port (default 3306).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// MySQL user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// MySQL password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Database name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    /// Profile name from ~/.config/tools4a/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Path to a YAML config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s). Comma-separated string for multi-hop, or a JSON array.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user (used when `tunnel = "ssh"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TunnelKind {
    Direct,
    Ssh,
}

/// Accepts either a single host string, a comma-separated string, or a JSON array.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SshJumpInput {
    Single(String),
    Multiple(Vec<String>),
}

impl SshJumpInput {
    pub fn into_jumps(self) -> Vec<String> {
        match self {
            SshJumpInput::Single(s) => s
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
            SshJumpInput::Multiple(v) => v.into_iter().filter(|s| !s.is_empty()).collect(),
        }
    }
}

/// Convert the JSON params into a fully-resolved `Config`, applying the
/// same priority order as the CLI: TOML profile (lowest) -> YAML file ->
/// explicit MCP fields (highest).
fn params_to_config(p: &MysqlExecParams) -> Result<Config> {
    let mut configs: Vec<Config> = Vec::new();

    if let Some(profile_name) = &p.profile {
        let toml_config = ConfigLoader::load_default_toml()?.ok_or_else(|| {
            Error::Config(format!(
                "profile '{}' requested but no ~/.config/tools4a/config.toml found",
                profile_name
            ))
        })?;
        let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
            Error::Config(format!(
                "profile '{}' not found in config.toml",
                profile_name
            ))
        })?;
        configs.push(profile_to_config(profile_cfg));
    }

    if let Some(path) = p.config.as_deref() {
        configs.push(ConfigLoader::load_yaml_file(path)?);
    }

    let tunnel_config = build_tunnel_config(p)?;
    configs.push(Config {
        service_type: Some(ServiceType::Mysql),
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

fn build_tunnel_config(p: &MysqlExecParams) -> Result<Option<TunnelConfig>> {
    let Some(kind) = &p.tunnel else {
        return Ok(None);
    };
    match kind {
        TunnelKind::Direct => {
            let stray = p.ssh_jump.is_some()
                || p.ssh_user.is_some()
                || p.ssh_password.is_some()
                || p.ssh_key_path.is_some()
                || p.ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = p
                .ssh_jump
                .clone()
                .map(SshJumpInput::into_jumps)
                .ok_or_else(|| {
                    Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
                })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = p.ssh_user.clone().ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password: p.ssh_password.clone(),
                ssh_key_path: p.ssh_key_path.clone(),
                ssh_port: p.ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the mysql_exec tool: params in, structured
/// result out. The MCP server wraps this with JSON-RPC plumbing.
pub async fn mysql_exec(params: MysqlExecParams) -> Result<ExecutionResult> {
    let query = params.query.clone();
    let allow_write = params.allow_write;
    let config = params_to_config(&params)?;
    let tunnel = config.tunnel.clone();
    let mut req = MysqlRequest::from_config(config, query)?;
    req.allow_write = allow_write;
    MysqlOrchestrator::execute(req, tunnel).await
}

/// JSON parameters for the `pgsql_exec` MCP tool. Mirrors the CLI's
/// `pgsql` subcommand args plus the global tunnel/config flags.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PgsqlExecParams {
    /// SQL query to execute.
    pub query: String,

    /// Allow write operations. Default false. When false the session
    /// also runs with `default_transaction_read_only = on`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,

    /// Pgsql host (overrides profile / yaml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Pgsql port (default 5432).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Pgsql user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// Pgsql password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Database name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    /// Profile name from ~/.config/tools4a/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Path to a YAML config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s). Comma-separated string for multi-hop, or a JSON array.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user (used when `tunnel = "ssh"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn pgsql_params_to_config(p: &PgsqlExecParams) -> Result<Config> {
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

    let tunnel_config = build_tunnel_config_for_pgsql(p)?;
    configs.push(Config {
        service_type: Some(ServiceType::Pgsql),
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

/// Same shape as the MySQL build_tunnel_config but reads from PgsqlExecParams.
fn build_tunnel_config_for_pgsql(p: &PgsqlExecParams) -> Result<Option<TunnelConfig>> {
    let Some(kind) = &p.tunnel else {
        return Ok(None);
    };
    match kind {
        TunnelKind::Direct => {
            let stray = p.ssh_jump.is_some()
                || p.ssh_user.is_some()
                || p.ssh_password.is_some()
                || p.ssh_key_path.is_some()
                || p.ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = p
                .ssh_jump
                .clone()
                .map(SshJumpInput::into_jumps)
                .ok_or_else(|| {
                    Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
                })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = p.ssh_user.clone().ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password: p.ssh_password.clone(),
                ssh_key_path: p.ssh_key_path.clone(),
                ssh_port: p.ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the pgsql_exec tool.
pub async fn pgsql_exec(params: PgsqlExecParams) -> Result<ExecutionResult> {
    let query = params.query.clone();
    let allow_write = params.allow_write;
    let config = pgsql_params_to_config(&params)?;
    let tunnel = config.tunnel.clone();
    let mut req = PgsqlRequest::from_config(config, query)?;
    req.allow_write = allow_write;
    PgsqlOrchestrator::execute(req, tunnel).await
}

/// JSON parameters for the `redis_exec` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RedisExecParams {
    /// Redis command to execute (e.g. "GET key" or "HSET h f1 v1").
    pub command: String,

    /// Redis host (overrides profile / yaml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Redis port (default 6379).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Redis password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Redis database number (default 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db: Option<u32>,

    /// Profile name from ~/.config/tools4a/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Path to a YAML config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn redis_params_to_config(p: &RedisExecParams) -> Result<Config> {
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

    let tunnel_config = build_tunnel_config_for_redis(p)?;
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
    });

    Ok(ConfigMerger::merge_multiple(configs))
}

/// Same shape as the MySQL build_tunnel_config but reads from RedisExecParams.
/// Refactor opportunity: extract a shared helper taking the SSH fields by reference.
fn build_tunnel_config_for_redis(p: &RedisExecParams) -> Result<Option<TunnelConfig>> {
    let Some(kind) = &p.tunnel else {
        return Ok(None);
    };
    match kind {
        TunnelKind::Direct => {
            let stray = p.ssh_jump.is_some()
                || p.ssh_user.is_some()
                || p.ssh_password.is_some()
                || p.ssh_key_path.is_some()
                || p.ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = p
                .ssh_jump
                .clone()
                .map(SshJumpInput::into_jumps)
                .ok_or_else(|| {
                    Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
                })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = p.ssh_user.clone().ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password: p.ssh_password.clone(),
                ssh_key_path: p.ssh_key_path.clone(),
                ssh_port: p.ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the redis_exec tool.
pub async fn redis_exec(params: RedisExecParams) -> Result<ExecutionResult> {
    let command = params.command.clone();
    let config = redis_params_to_config(&params)?;
    let tunnel = config.tunnel.clone();
    let req = RedisRequest::from_config(config, command)?;
    RedisOrchestrator::execute(req, tunnel).await
}

/// JSON parameters for the `mongo_exec` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MongoExecParams {
    /// Mongo command as a JSON object (e.g. `{"find":"users","filter":{}}`).
    pub command: String,

    /// Allow write commands (insert/update/delete/drop/findAndModify/
    /// aggregate-with-$out etc.). Default false. Mongo has no per-session
    /// read-only mode, so this is the only guard.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,

    /// Mongo host (overrides profile / yaml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Mongo port (default 27017).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Mongo user (optional — auth is optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// Mongo password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Database name (required for runCommand).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    /// Profile name from ~/.config/tools4a/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Path to a YAML config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn mongo_params_to_config(p: &MongoExecParams) -> Result<Config> {
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

    let tunnel_config = build_tunnel_config_for_mongo(p)?;
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

/// Same shape as the Redis build_tunnel_config_for_redis but reads MongoExecParams.
fn build_tunnel_config_for_mongo(p: &MongoExecParams) -> Result<Option<TunnelConfig>> {
    let Some(kind) = &p.tunnel else {
        return Ok(None);
    };
    match kind {
        TunnelKind::Direct => {
            let stray = p.ssh_jump.is_some()
                || p.ssh_user.is_some()
                || p.ssh_password.is_some()
                || p.ssh_key_path.is_some()
                || p.ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = p
                .ssh_jump
                .clone()
                .map(SshJumpInput::into_jumps)
                .ok_or_else(|| {
                    Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
                })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = p.ssh_user.clone().ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password: p.ssh_password.clone(),
                ssh_key_path: p.ssh_key_path.clone(),
                ssh_port: p.ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the mongo_exec tool.
pub async fn mongo_exec(params: MongoExecParams) -> Result<ExecutionResult> {
    let command = params.command.clone();
    let allow_write = params.allow_write;
    let config = mongo_params_to_config(&params)?;
    let tunnel = config.tunnel.clone();
    let mut req = MongoRequest::from_config(config, command)?;
    req.allow_write = allow_write;
    MongoOrchestrator::execute(req, tunnel).await
}

/// JSON parameters for the `http_exec` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HttpExecParams {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS).
    pub method: String,

    /// Full URL (http:// or https://).
    pub url: String,

    /// Extra headers as a list of `Name: Value` strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<String>,

    /// Request body (raw string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,

    /// Set Content-Type: application/json (does not transform the body).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub json: bool,

    /// `Authorization: Bearer <TOKEN>` shortcut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer: Option<String>,

    /// HTTP Basic auth as `user:password`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basic: Option<String>,

    /// Accept invalid TLS certificates (self-signed). DANGER.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure: bool,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn http_params_to_request_and_tunnel(
    p: HttpExecParams,
) -> Result<(HttpRequestSpec, Option<TunnelConfig>)> {
    let mut header_pairs: Vec<(String, String)> = Vec::new();
    for raw in &p.headers {
        let (name, value) = raw.split_once(':').ok_or_else(|| {
            Error::Config(format!(
                "header '{raw}' must be 'Name: Value' (missing ':')"
            ))
        })?;
        header_pairs.push((name.trim().to_string(), value.trim().to_string()));
    }
    if p.json {
        header_pairs.push(("Content-Type".to_string(), "application/json".to_string()));
    }

    let auth = match (p.bearer, p.basic) {
        (Some(token), None) => HttpAuth::Bearer(token),
        (None, Some(creds)) => {
            let (user, password) = creds
                .split_once(':')
                .ok_or_else(|| Error::Config("basic must be 'user:password'".to_string()))?;
            HttpAuth::Basic {
                user: user.to_string(),
                password: password.to_string(),
            }
        }
        (None, None) => HttpAuth::None,
        (Some(_), Some(_)) => {
            return Err(Error::Config(
                "bearer and basic are mutually exclusive".to_string(),
            ));
        }
    };

    let req = HttpRequestSpec {
        method: p.method,
        url: p.url,
        headers: header_pairs,
        body: p.data.map(|s| s.into_bytes()),
        auth,
        insecure: p.insecure,
    };

    let tunnel_config = build_tunnel_config_for_http(
        p.tunnel,
        p.ssh_jump,
        p.ssh_user,
        p.ssh_password,
        p.ssh_key_path,
        p.ssh_port,
    )?;

    Ok((req, tunnel_config))
}

fn build_tunnel_config_for_http(
    kind: Option<TunnelKind>,
    ssh_jump: Option<SshJumpInput>,
    ssh_user: Option<String>,
    ssh_password: Option<String>,
    ssh_key_path: Option<String>,
    ssh_port: Option<u16>,
) -> Result<Option<TunnelConfig>> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    match kind {
        TunnelKind::Direct => {
            let stray = ssh_jump.is_some()
                || ssh_user.is_some()
                || ssh_password.is_some()
                || ssh_key_path.is_some()
                || ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = ssh_jump.map(SshJumpInput::into_jumps).ok_or_else(|| {
                Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
            })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = ssh_user.ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port: ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the http_exec tool.
pub async fn http_exec(params: HttpExecParams) -> Result<ExecutionResult> {
    let (req, tunnel_config) = http_params_to_request_and_tunnel(params)?;
    HttpOrchestrator::execute(req, tunnel_config).await
}

/// JSON parameters for the `ssh_exec` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SshExecParams {
    /// Shell command to execute on the target.
    pub command: String,

    /// Target SSH host.
    pub host: String,

    /// Target SSH port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Target SSH user.
    pub user: String,

    /// Target SSH password (mutually exclusive with key_path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Target SSH key path. Unencrypted keys only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH jump key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn ssh_params_to_request_and_tunnel(
    p: SshExecParams,
) -> Result<(SshExecRequest, Option<TunnelConfig>)> {
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
    };

    let tunnel_config = build_tunnel_config_for_ssh_direct(
        p.tunnel,
        p.ssh_jump,
        p.ssh_user,
        p.ssh_password,
        p.ssh_key_path,
        p.ssh_port,
    )?;

    Ok((req, tunnel_config))
}

fn build_tunnel_config_for_ssh_direct(
    kind: Option<TunnelKind>,
    ssh_jump: Option<SshJumpInput>,
    ssh_user: Option<String>,
    ssh_password: Option<String>,
    ssh_key_path: Option<String>,
    ssh_port: Option<u16>,
) -> Result<Option<TunnelConfig>> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    match kind {
        TunnelKind::Direct => {
            let stray = ssh_jump.is_some()
                || ssh_user.is_some()
                || ssh_password.is_some()
                || ssh_key_path.is_some()
                || ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = ssh_jump.map(SshJumpInput::into_jumps).ok_or_else(|| {
                Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
            })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = ssh_user.ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port: ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the ssh_exec tool.
pub async fn ssh_exec(params: SshExecParams) -> Result<ExecutionResult> {
    let (req, tunnel_config) = ssh_params_to_request_and_tunnel(params)?;
    SshDirectOrchestrator::execute(req, tunnel_config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_params() -> MysqlExecParams {
        MysqlExecParams {
            query: "SELECT 1".to_string(),
            allow_write: false,
            host: None,
            port: None,
            user: None,
            password: None,
            database: None,
            profile: None,
            config: None,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        }
    }

    #[test]
    fn test_explicit_fields_become_config() {
        let p = MysqlExecParams {
            host: Some("db.example.com".into()),
            port: Some(3307),
            user: Some("alice".into()),
            ..empty_params()
        };
        let cfg = params_to_config(&p).unwrap();
        assert_eq!(cfg.host.as_deref(), Some("db.example.com"));
        assert_eq!(cfg.port, Some(3307));
        assert_eq!(cfg.user.as_deref(), Some("alice"));
    }

    #[test]
    fn test_tunnel_ssh_with_string_jump_splits_commas() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Ssh),
            ssh_jump: Some(SshJumpInput::Single("b1.com,b2.com".into())),
            ssh_user: Some("admin".into()),
            ..empty_params()
        };
        let cfg = params_to_config(&p).unwrap();
        match cfg.tunnel {
            Some(TunnelConfig::Ssh { ssh_jumps, .. }) => {
                assert_eq!(ssh_jumps, vec!["b1.com".to_string(), "b2.com".to_string()]);
            }
            other => panic!("expected Ssh tunnel, got {other:?}"),
        }
    }

    #[test]
    fn test_tunnel_ssh_with_array_jump() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Ssh),
            ssh_jump: Some(SshJumpInput::Multiple(vec!["b1".into(), "b2".into()])),
            ssh_user: Some("admin".into()),
            ..empty_params()
        };
        let cfg = params_to_config(&p).unwrap();
        match cfg.tunnel {
            Some(TunnelConfig::Ssh { ssh_jumps, .. }) => {
                assert_eq!(ssh_jumps, vec!["b1".to_string(), "b2".to_string()]);
            }
            other => panic!("expected Ssh tunnel, got {other:?}"),
        }
    }

    #[test]
    fn test_tunnel_direct_with_stray_ssh_field_errors() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Direct),
            ssh_jump: Some(SshJumpInput::Single("bastion".into())),
            ..empty_params()
        };
        let err = params_to_config(&p).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("ssh_*")));
    }

    #[test]
    fn test_tunnel_ssh_without_jump_errors() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Ssh),
            ssh_user: Some("admin".into()),
            ..empty_params()
        };
        let err = params_to_config(&p).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("ssh_jump")));
    }

    #[test]
    fn test_mysql_allow_write_default_false_via_serde() {
        // allow_write should default to false when omitted in JSON.
        let p: MysqlExecParams =
            serde_json::from_value(serde_json::json!({"query": "SELECT 1"})).unwrap();
        assert!(!p.allow_write);
    }

    #[test]
    fn test_redis_explicit_fields_become_config() {
        let p = RedisExecParams {
            command: "GET key".to_string(),
            host: Some("redis.internal".into()),
            port: Some(6380),
            password: Some("pwd".into()),
            db: Some(2),
            profile: None,
            config: None,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let cfg = redis_params_to_config(&p).unwrap();
        assert_eq!(cfg.host.as_deref(), Some("redis.internal"));
        assert_eq!(cfg.port, Some(6380));
        assert_eq!(cfg.password.as_deref(), Some("pwd"));
        assert_eq!(cfg.db, Some(2));
        assert_eq!(cfg.service_type, Some(ServiceType::Redis));
    }

    #[test]
    fn test_http_params_to_request_basic() {
        let p = HttpExecParams {
            method: "POST".into(),
            url: "https://api.example.com/x".into(),
            headers: vec!["X-Foo: bar".into()],
            data: Some(r#"{"a":1}"#.into()),
            json: true,
            bearer: Some("tok".into()),
            basic: None,
            insecure: false,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let (req, tunnel) = http_params_to_request_and_tunnel(p).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "https://api.example.com/x");
        assert!(
            req.headers
                .contains(&("X-Foo".to_string(), "bar".to_string()))
        );
        assert!(
            req.headers
                .contains(&("Content-Type".to_string(), "application/json".to_string()))
        );
        assert_eq!(req.body.as_deref(), Some(r#"{"a":1}"#.as_bytes()));
        match req.auth {
            HttpAuth::Bearer(t) => assert_eq!(t, "tok"),
            other => panic!("expected Bearer, got {other:?}"),
        }
        assert!(tunnel.is_none());
    }

    #[test]
    fn test_ssh_params_to_request_basic() {
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
        };
        let (req, tunnel) = ssh_params_to_request_and_tunnel(p).unwrap();
        assert_eq!(req.command, "uptime");
        assert_eq!(req.host, "server.com");
        assert_eq!(req.port, 22);
        assert_eq!(req.user, "admin");
        assert_eq!(req.password.as_deref(), Some("pwd"));
        assert!(req.key_path.is_none());
        assert!(tunnel.is_none());
    }

    #[test]
    fn test_ssh_params_password_and_key_mutex() {
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
        };
        let err = ssh_params_to_request_and_tunnel(p).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("mutually exclusive")));
    }
}
