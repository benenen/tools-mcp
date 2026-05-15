//! Helpers shared across the per-service handler submodules:
//! - 3-layer Config merge (TOML profile -> YAML file -> CLI args)
//! - `cli_to_tunnel_config` + Profile->Config converter
//! - operator-side `max_timeout_secs` lookup
//! - stderr warnings sink

use crate::cli::{Cli, TunnelKind};
use tools4a_core::config::{Config, ConfigLoader, ConfigMerger, Profile, ServiceType, TomlConfig};
use tools4a_core::{Error, ExecutionResult, Result, TunnelConfig};

/// 3-layer config build for typed-DB services (mysql/pgsql/clickhouse/mongo).
#[allow(clippy::too_many_arguments)]
pub(super) fn build_config(
    cli: &Cli,
    service_type: ServiceType,
    host: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    password: Option<String>,
    database: Option<String>,
    key_path: Option<String>,
    profile: Option<String>,
) -> Result<Config> {
    let mut configs: Vec<Config> = Vec::new();

    // 1. Default TOML profile (if --profile=NAME and ~/.config/tools4a/config.toml exists)
    if let Some(profile_name) = &profile {
        if let Some(toml_config) = ConfigLoader::load_default_toml()? {
            let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
                Error::Config(format!("profile '{profile_name}' not found in config.toml"))
            })?;
            configs.push(profile_to_config(profile_cfg));
        } else {
            return Err(Error::Config(format!(
                "profile '{profile_name}' requested but no ~/.config/tools4a/config.toml found"
            )));
        }
    }

    // 2. YAML config file
    if let Some(config_path) = cli.config.as_deref() {
        configs.push(ConfigLoader::load_yaml_file(config_path)?);
    }

    // 3. CLI arguments (highest priority)
    let tunnel_config = cli_to_tunnel_config(cli)?;
    configs.push(Config {
        service_type: Some(service_type),
        host,
        port,
        user,
        password,
        database,
        db: None,
        key_path,
        tunnel: tunnel_config,
        timeout_secs: cli.timeout,
    });

    Ok(ConfigMerger::merge_multiple(configs))
}

/// Redis-flavored Config build (different field set: `db` instead of `database`,
/// no `user`).
pub(super) fn build_config_redis(
    cli: &Cli,
    host: Option<String>,
    port: Option<u16>,
    password: Option<String>,
    db: Option<u32>,
    profile: Option<String>,
) -> Result<Config> {
    let mut configs: Vec<Config> = Vec::new();

    if let Some(profile_name) = &profile {
        if let Some(toml_config) = ConfigLoader::load_default_toml()? {
            let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
                Error::Config(format!("profile '{profile_name}' not found in config.toml"))
            })?;
            configs.push(profile_to_config(profile_cfg));
        } else {
            return Err(Error::Config(format!(
                "profile '{profile_name}' requested but no ~/.config/tools4a/config.toml found"
            )));
        }
    }

    if let Some(config_path) = cli.config.as_deref() {
        configs.push(ConfigLoader::load_yaml_file(config_path)?);
    }

    let tunnel_config = cli_to_tunnel_config(cli)?;
    configs.push(Config {
        service_type: Some(ServiceType::Redis),
        host,
        port,
        user: None,
        password,
        database: None,
        db,
        key_path: None,
        tunnel: tunnel_config,
        timeout_secs: cli.timeout,
    });

    Ok(ConfigMerger::merge_multiple(configs))
}

/// Read TOML `[defaults].max_timeout_secs` once per CLI invocation. Env var
/// `TOOLS4A_MAX_TIMEOUT_SECS` still takes precedence at the orchestrator layer.
pub(super) fn load_max_timeout_secs() -> Result<Option<u64>> {
    Ok(ConfigLoader::load_default_toml()?.and_then(|t: TomlConfig| t.defaults.max_timeout_secs))
}

/// Emit non-fatal advisories (e.g. timeout-clamp notices) to stderr so they
/// don't get tangled up with the result table on stdout.
pub(super) fn print_warnings(result: &ExecutionResult) {
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
}

/// Convert top-level CLI `--tunnel` + `--ssh-*` flags into a `TunnelConfig`.
pub(super) fn cli_to_tunnel_config(cli: &Cli) -> Result<Option<TunnelConfig>> {
    let Some(kind) = cli.tunnel else {
        return Ok(None);
    };
    let ssh = &cli.ssh;
    match kind {
        TunnelKind::Direct => {
            let stray_ssh = ssh.ssh_jump.is_some()
                || ssh.ssh_user.is_some()
                || ssh.ssh_password.is_some()
                || ssh.ssh_key_path.is_some()
                || ssh.ssh_port.is_some();
            if stray_ssh {
                return Err(Error::Config(
                    "SSH options (--ssh-*) are only valid with --tunnel=ssh".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let raw_jump = ssh.ssh_jump.clone().ok_or_else(|| {
                Error::Config("--ssh-jump is required when --tunnel=ssh".to_string())
            })?;
            let ssh_jumps: Vec<String> = raw_jump
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if ssh_jumps.is_empty() {
                return Err(Error::Config("--ssh-jump must not be empty".to_string()));
            }
            let ssh_user = ssh.ssh_user.clone().ok_or_else(|| {
                Error::Config("--ssh-user is required when --tunnel=ssh".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps,
                ssh_user,
                ssh_password: ssh.ssh_password.clone(),
                ssh_key_path: ssh.ssh_key_path.clone(),
                ssh_port: ssh.ssh_port.unwrap_or(22),
            }))
        }
    }
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
