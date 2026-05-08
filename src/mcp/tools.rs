use crate::config::{Config, ConfigLoader, ServiceType, TunnelConfig};
use crate::core::mysql;
use crate::output::ExecutionResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tools_mcp_core::{Error, Result};

/// JSON parameters for the `mysql_exec` MCP tool. Mirrors the CLI's
/// `mysql` subcommand args plus the global tunnel/config flags, so an
/// AI assistant can invoke any MySQL query the CLI can run.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MysqlExecParams {
    /// SQL query to execute.
    pub query: String,

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

    /// Profile name from ~/.config/tools-mcp/config.toml.
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
                "profile '{}' requested but no ~/.config/tools-mcp/config.toml found",
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

    Ok(crate::config::ConfigMerger::merge_multiple(configs))
}

fn profile_to_config(profile: &crate::config::Profile) -> Config {
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
    let config = params_to_config(&params)?;
    mysql::execute(config, &query).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_params() -> MysqlExecParams {
        MysqlExecParams {
            query: "SELECT 1".to_string(),
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
}
