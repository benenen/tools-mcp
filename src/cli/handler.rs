use crate::cli::{Cli, Commands, TunnelKind};
use crate::config::{Config, ConfigLoader, ConfigMerger, ServiceType, TunnelConfig};
use crate::connection::{Connection, MySQLConnection};
use crate::error::{Error, Result};
use crate::executor::MySQLExecutor;
use crate::output::CliFormatter;
use crate::tunnel::{DirectTunnel, SshTunnel, Tunnel};

pub struct CliHandler;

impl CliHandler {
    pub async fn handle(cli: Cli) -> Result<()> {
        match cli.command.clone() {
            Some(Commands::Mysql {
                query,
                host,
                port,
                user,
                password,
                database,
                profile,
            }) => {
                let config = Self::build_config(
                    &cli,
                    ServiceType::Mysql,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None, // key_path is not a MySQL flag
                    profile,
                )?;

                Self::execute_mysql(&query, config).await
            }
            None => Err(Error::Config(
                "No command specified. Run with --help for usage.".to_string(),
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_config(
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

        // 1. Default TOML profile (if --profile=NAME and ~/.config/tools-mcp/config.toml exists)
        if let Some(profile_name) = &profile {
            if let Some(toml_config) = ConfigLoader::load_default_toml()? {
                let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
                    Error::Config(format!(
                        "profile '{}' not found in config.toml",
                        profile_name
                    ))
                })?;
                configs.push(Self::profile_to_config(profile_cfg));
            } else {
                return Err(Error::Config(format!(
                    "profile '{}' requested but no ~/.config/tools-mcp/config.toml found",
                    profile_name
                )));
            }
        }

        // 2. YAML config file (if --config=PATH)
        if let Some(config_path) = cli.config.as_deref() {
            let yaml_config = ConfigLoader::load_yaml_file(config_path)?;
            configs.push(yaml_config);
        }

        // 3. CLI arguments (highest priority)
        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        configs.push(Config {
            service_type: Some(service_type),
            host,
            port,
            user,
            password,
            database,
            key_path,
            tunnel: tunnel_config,
        });

        Ok(ConfigMerger::merge_multiple(configs))
    }

    fn cli_to_tunnel_config(cli: &Cli) -> Result<Option<TunnelConfig>> {
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

    fn profile_to_config(profile: &crate::config::Profile) -> Config {
        Config {
            service_type: Some(profile.service_type.clone()),
            host: profile.host.clone(),
            port: profile.port,
            user: profile.user.clone(),
            password: profile.password.clone(),
            database: profile.database.clone(),
            key_path: profile.key_path.clone(),
            tunnel: profile.tunnel.clone(),
        }
    }

    async fn execute_mysql(query: &str, config: Config) -> Result<()> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("MySQL host is required".to_string()))?;
        let port = config.port.unwrap_or(3306);
        let user = config
            .user
            .ok_or_else(|| Error::Config("MySQL user is required".to_string()))?;

        let tunnel: Box<dyn Tunnel> = match config.tunnel {
            None | Some(TunnelConfig::Direct) => Box::new(DirectTunnel::new(host, port)),
            Some(TunnelConfig::Ssh {
                ssh_jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
            }) => {
                let key_path = ssh_key_path.map(std::path::PathBuf::from);
                Box::new(SshTunnel::new(
                    ssh_jumps,
                    ssh_user,
                    ssh_password,
                    key_path,
                    ssh_port,
                    host,
                    port,
                )?)
            }
        };

        let mut conn = MySQLConnection::new(tunnel, user, config.password, config.database);
        let exec_result = MySQLExecutor::execute(&mut conn, query).await;
        // Always tear down the tunnel + pool, even on query error.
        let _ = conn.disconnect().await;
        let output = CliFormatter::format(&exec_result?);
        println!("{output}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_handler_new() {
        let _handler = CliHandler;
    }
}
