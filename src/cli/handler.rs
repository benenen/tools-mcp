use crate::cli::{Cli, Commands, TunnelKind};
use crate::output::CliFormatter;
use tools_mcp_core::{Error, Result, Service, TunnelConfig};
use tools_mcp_orchestrator::config::{Config, ConfigLoader, ConfigMerger, ServiceType};
use tools_mcp_orchestrator::{
    HttpAuth, HttpOrchestrator, HttpRequestSpec, MongoOrchestrator, MongoRequest,
    MysqlOrchestrator, MysqlRequest, PgsqlOrchestrator, PgsqlRequest, RedisOrchestrator,
    RedisRequest, SshDirectOrchestrator, SshExecRequest,
};

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
            Some(Commands::Pgsql {
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
                    ServiceType::Pgsql,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None, // key_path is not a Pgsql flag
                    profile,
                )?;
                Self::execute_pgsql(&query, config).await
            }
            Some(Commands::Redis {
                command,
                host,
                port,
                password,
                db,
                profile,
            }) => {
                let config = Self::build_config_redis(&cli, host, port, password, db, profile)?;
                Self::execute_redis(&command, config).await
            }
            Some(Commands::Mongo {
                command,
                host,
                port,
                user,
                password,
                database,
                profile,
            }) => {
                let config = Self::build_config(
                    &cli,
                    ServiceType::Mongo,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None,
                    profile,
                )?;
                Self::execute_mongo(&command, config).await
            }
            Some(Commands::Http {
                method,
                url,
                headers,
                data,
                data_file,
                json,
                bearer,
                basic,
                insecure,
                include_headers,
            }) => {
                Self::execute_http(
                    &cli,
                    method,
                    url,
                    headers,
                    data,
                    data_file,
                    json,
                    bearer,
                    basic,
                    insecure,
                    include_headers,
                )
                .await
            }
            Some(Commands::Ssh {
                command,
                host,
                port,
                user,
                password,
                key_path,
                include_headers,
            }) => {
                Self::execute_ssh(
                    &cli,
                    command,
                    host,
                    port,
                    user,
                    password,
                    key_path,
                    include_headers,
                )
                .await
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
            db: None,
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

    fn profile_to_config(profile: &tools_mcp_orchestrator::config::Profile) -> Config {
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

    async fn execute_mysql(query: &str, config: Config) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let req = MysqlRequest::from_config(config, query.to_string())?;
        let result = MysqlOrchestrator::execute(req, tunnel).await?;
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }

    async fn execute_pgsql(query: &str, config: Config) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let req = PgsqlRequest::from_config(config, query.to_string())?;
        let result = PgsqlOrchestrator::execute(req, tunnel).await?;
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }

    async fn execute_mongo(command: &str, config: Config) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let req = MongoRequest::from_config(config, command.to_string())?;
        let result = MongoOrchestrator::execute(req, tunnel).await?;
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }

    fn build_config_redis(
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
                configs.push(Self::profile_to_config(profile_cfg));
            } else {
                return Err(Error::Config(format!(
                    "profile '{profile_name}' requested but no ~/.config/tools-mcp/config.toml found"
                )));
            }
        }

        if let Some(config_path) = cli.config.as_deref() {
            configs.push(ConfigLoader::load_yaml_file(config_path)?);
        }

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
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
        });

        Ok(ConfigMerger::merge_multiple(configs))
    }

    async fn execute_redis(command: &str, config: Config) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let req = RedisRequest::from_config(config, command.to_string())?;
        let result = RedisOrchestrator::execute(req, tunnel).await?;
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_ssh(
        cli: &Cli,
        command: String,
        host: String,
        port: u16,
        user: String,
        password: Option<String>,
        key_path: Option<std::path::PathBuf>,
        include_headers: bool,
    ) -> Result<()> {
        let req = SshExecRequest {
            host,
            port,
            user,
            password,
            key_path,
            command,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result = SshDirectOrchestrator::execute(req, tunnel_config).await?;

        if include_headers {
            println!("{}", CliFormatter::format(&result));
            return Ok(());
        }

        // Default: print stdout to stdout, stderr to stderr, exit with the
        // remote exit code.
        let mut exit_code: i32 = 0;
        for row in &result.rows {
            if row.len() < 2 {
                continue;
            }
            match row[0].as_str() {
                "exit_code" => {
                    exit_code = row[1].parse().unwrap_or(0);
                }
                "stdout" => {
                    use std::io::Write;
                    let _ = std::io::stdout().write_all(row[1].as_bytes());
                }
                "stderr" => {
                    use std::io::Write;
                    let _ = std::io::stderr().write_all(row[1].as_bytes());
                }
                _ => {}
            }
        }
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_http(
        cli: &Cli,
        method: String,
        url: String,
        headers: Vec<String>,
        data: Option<String>,
        data_file: Option<std::path::PathBuf>,
        json: bool,
        bearer: Option<String>,
        basic: Option<String>,
        insecure: bool,
        include_headers: bool,
    ) -> Result<()> {
        // Parse `Name: Value` strings into pairs.
        let mut header_pairs: Vec<(String, String)> = Vec::new();
        for raw in headers {
            let (name, value) = raw.split_once(':').ok_or_else(|| {
                Error::Config(format!(
                    "--header '{raw}' must be 'Name: Value' (missing ':')"
                ))
            })?;
            header_pairs.push((name.trim().to_string(), value.trim().to_string()));
        }
        if json {
            header_pairs.push(("Content-Type".to_string(), "application/json".to_string()));
        }

        // Body
        let body: Option<Vec<u8>> = match (data, data_file) {
            (Some(s), None) => Some(s.into_bytes()),
            (None, Some(path)) => {
                let bytes = std::fs::read(&path).map_err(|e| {
                    Error::Config(format!("cannot read --data-file '{}': {e}", path.display()))
                })?;
                Some(bytes)
            }
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
        };

        // Auth
        let auth = match (bearer, basic) {
            (Some(token), None) => HttpAuth::Bearer(token),
            (None, Some(creds)) => {
                let (user, password) = creds
                    .split_once(':')
                    .ok_or_else(|| Error::Config("--basic must be 'user:password'".to_string()))?;
                HttpAuth::Basic {
                    user: user.to_string(),
                    password: password.to_string(),
                }
            }
            (None, None) => HttpAuth::None,
            (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
        };

        let req = HttpRequestSpec {
            method,
            url,
            headers: header_pairs,
            body,
            auth,
            insecure,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result = HttpOrchestrator::execute(req, tunnel_config).await?;

        if include_headers {
            println!("{}", CliFormatter::format(&result));
        } else {
            // Default: print just the body row (the last row, by construction).
            if let Some(body_row) = result.rows.last() {
                if body_row.len() >= 2 && body_row[0] == "body" {
                    println!("{}", body_row[1]);
                } else {
                    // Fallback if row layout drifts: print the whole table.
                    println!("{}", CliFormatter::format(&result));
                }
            }
        }
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
