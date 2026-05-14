use crate::cli::{Cli, Commands, DockerCommand, RabbitmqCommand, TunnelKind, TunnelServeType};
use crate::output::CliFormatter;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use tools4a_browser::{BrowserOrchestrator, BrowserRequest};
use tools4a_clickhouse::{ClickhouseOrchestrator, ClickhouseRequest};
use tools4a_core::config::{Config, ConfigLoader, ConfigMerger, ServiceType, TomlConfig};
use tools4a_core::{
    Error, ExecutionResult, Result, Service, SocksTunnel, SshTunnel, StreamLocalTunnel, Tunnel,
    TunnelConfig,
};
use tools4a_docker::{DockerAction, DockerOrchestrator, DockerRequest};
use tools4a_http::{HttpAuth, HttpOrchestrator, HttpRequestSpec};
use tools4a_mongo::{MongoOrchestrator, MongoRequest};
use tools4a_mysql::{MysqlOrchestrator, MysqlRequest};
use tools4a_pgsql::{PgsqlOrchestrator, PgsqlRequest};
use tools4a_rabbitmq::{
    RabbitmqAction, RabbitmqOrchestrator, RabbitmqRequest, orchestrator::default_port_for,
};
use tools4a_redis::{RedisOrchestrator, RedisRequest};
use tools4a_ssh::{SshDirectOrchestrator, SshExecRequest};

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
                allow_write,
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

                Self::execute_mysql(&query, config, allow_write).await
            }
            Some(Commands::Pgsql {
                query,
                host,
                port,
                user,
                password,
                database,
                profile,
                allow_write,
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
                Self::execute_pgsql(&query, config, allow_write).await
            }
            Some(Commands::Clickhouse {
                query,
                host,
                port,
                user,
                password,
                database,
                profile,
                allow_write,
            }) => {
                let config = Self::build_config(
                    &cli,
                    ServiceType::Clickhouse,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None, // key_path is not a Clickhouse flag
                    profile,
                )?;
                Self::execute_clickhouse(&query, config, allow_write).await
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
                allow_write,
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
                Self::execute_mongo(&command, config, allow_write).await
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
            Some(Commands::Browser {
                subcommand,
                args,
                session,
                proxy,
                proxy_bypass,
                browser_args,
                bin,
                include_headers,
            }) => {
                Self::execute_browser(
                    &cli,
                    subcommand,
                    args,
                    session,
                    proxy,
                    proxy_bypass,
                    browser_args,
                    bin,
                    include_headers,
                )
                .await
            }
            Some(Commands::Docker {
                docker_host,
                unix_socket,
                action,
            }) => Self::execute_docker(&cli, docker_host, unix_socket, action).await,
            Some(Commands::Rabbitmq {
                host,
                scheme,
                port,
                user,
                password,
                insecure,
                action,
            }) => {
                Self::execute_rabbitmq(&cli, host, scheme, port, user, password, insecure, action)
                    .await
            }
            Some(Commands::TunnelServe {
                kind,
                listen,
                ssh_jump,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
                target_host,
                target_port,
                remote_socket,
            }) => {
                Self::execute_tunnel_serve(
                    kind,
                    listen,
                    ssh_jump,
                    ssh_user,
                    ssh_password,
                    ssh_key_path,
                    ssh_port,
                    target_host,
                    target_port,
                    remote_socket,
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

        // 1. Default TOML profile (if --profile=NAME and ~/.config/tools4a/config.toml exists)
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
                    "profile '{}' requested but no ~/.config/tools4a/config.toml found",
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
            timeout_secs: cli.timeout,
        });

        Ok(ConfigMerger::merge_multiple(configs))
    }

    /// Read TOML `[defaults].max_timeout_secs` once for the current CLI
    /// invocation. Each service-execute helper consults this so the
    /// orchestrator's resolver can honor the operator-side cap. Env var
    /// `TOOLS4A_MAX_TIMEOUT_SECS` still takes precedence over the value
    /// pulled here.
    fn load_max_timeout_secs() -> Result<Option<u64>> {
        Ok(
            ConfigLoader::load_default_toml()?
                .and_then(|t: TomlConfig| t.defaults.max_timeout_secs),
        )
    }

    /// Emit non-fatal advisories (e.g. timeout-clamp notices) to stderr
    /// so they don't get tangled up with the result table on stdout.
    fn print_warnings(result: &ExecutionResult) {
        for w in &result.warnings {
            eprintln!("warning: {w}");
        }
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

    fn profile_to_config(profile: &tools4a_core::config::Profile) -> Config {
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

    async fn execute_mysql(query: &str, config: Config, allow_write: bool) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let max_timeout_secs = Self::load_max_timeout_secs()?;
        let mut req = MysqlRequest::from_config(config, query.to_string())?;
        req.allow_write = allow_write;
        req.max_timeout_secs = max_timeout_secs;
        let result = MysqlOrchestrator::execute(req, tunnel).await?;
        Self::print_warnings(&result);
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }

    async fn execute_pgsql(query: &str, config: Config, allow_write: bool) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let max_timeout_secs = Self::load_max_timeout_secs()?;
        let mut req = PgsqlRequest::from_config(config, query.to_string())?;
        req.allow_write = allow_write;
        req.max_timeout_secs = max_timeout_secs;
        let result = PgsqlOrchestrator::execute(req, tunnel).await?;
        Self::print_warnings(&result);
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }

    async fn execute_clickhouse(query: &str, config: Config, allow_write: bool) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let max_timeout_secs = Self::load_max_timeout_secs()?;
        let mut req = ClickhouseRequest::from_config(config, query.to_string())?;
        req.allow_write = allow_write;
        req.max_timeout_secs = max_timeout_secs;
        let result = ClickhouseOrchestrator::execute(req, tunnel).await?;
        Self::print_warnings(&result);
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }

    async fn execute_mongo(command: &str, config: Config, allow_write: bool) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let max_timeout_secs = Self::load_max_timeout_secs()?;
        let mut req = MongoRequest::from_config(config, command.to_string())?;
        req.allow_write = allow_write;
        req.max_timeout_secs = max_timeout_secs;
        let result = MongoOrchestrator::execute(req, tunnel).await?;
        Self::print_warnings(&result);
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
                    "profile '{profile_name}' requested but no ~/.config/tools4a/config.toml found"
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
            timeout_secs: cli.timeout,
        });

        Ok(ConfigMerger::merge_multiple(configs))
    }

    async fn execute_redis(command: &str, config: Config) -> Result<()> {
        let tunnel = config.tunnel.clone();
        let max_timeout_secs = Self::load_max_timeout_secs()?;
        let mut req = RedisRequest::from_config(config, command.to_string())?;
        req.max_timeout_secs = max_timeout_secs;
        let result = RedisOrchestrator::execute(req, tunnel).await?;
        Self::print_warnings(&result);
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
        let max_timeout_secs = Self::load_max_timeout_secs()?;
        let req = SshExecRequest {
            host,
            port,
            user,
            password,
            key_path,
            command,
            timeout_secs: cli.timeout,
            max_timeout_secs,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result = SshDirectOrchestrator::execute(req, tunnel_config).await?;
        Self::print_warnings(&result);

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

        let max_timeout_secs = Self::load_max_timeout_secs()?;
        let req = HttpRequestSpec {
            method,
            url,
            headers: header_pairs,
            body,
            auth,
            insecure,
            timeout_secs: cli.timeout,
            max_timeout_secs,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result = HttpOrchestrator::execute(req, tunnel_config).await?;
        Self::print_warnings(&result);

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

    #[allow(clippy::too_many_arguments)]
    async fn execute_browser(
        cli: &Cli,
        subcommand: String,
        args: Vec<String>,
        session: Option<String>,
        proxy: Option<String>,
        proxy_bypass: Option<String>,
        browser_args: Option<String>,
        bin: Option<std::path::PathBuf>,
        include_headers: bool,
    ) -> Result<()> {
        let req = BrowserRequest {
            subcommand,
            args,
            session,
            proxy,
            proxy_bypass,
            browser_args,
            bin,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result = BrowserOrchestrator::execute(req, tunnel_config).await?;
        Self::print_warnings(&result);

        if include_headers {
            println!("{}", CliFormatter::format(&result));
            return Ok(());
        }

        // Default: stream stdout/stderr, exit with the agent-browser code.
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

    async fn execute_docker(
        cli: &Cli,
        docker_host: Option<String>,
        unix_socket: Option<String>,
        action: DockerCommand,
    ) -> Result<()> {
        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let (action, allow_write) = match action {
            DockerCommand::Ps {
                all,
                limit,
                filters,
            } => {
                let filters_map = parse_kv_filters(&filters)?;
                (
                    DockerAction::Ps {
                        all,
                        limit,
                        filters: filters_map,
                    },
                    false,
                )
            }
            DockerCommand::Inspect { container } => (DockerAction::Inspect { container }, false),
            DockerCommand::Logs {
                container,
                tail,
                stdout,
                stderr,
                timestamps,
                since,
            } => (
                DockerAction::Logs {
                    container,
                    tail: Some(tail),
                    stdout,
                    stderr,
                    timestamps,
                    since,
                },
                false,
            ),
            DockerCommand::Stats { container } => (DockerAction::Stats { container }, false),
            DockerCommand::Top { container, ps_args } => {
                (DockerAction::Top { container, ps_args }, false)
            }
            DockerCommand::Run {
                container,
                cmd,
                user,
                working_dir,
                env,
                privileged,
                allow_write,
            } => (
                DockerAction::Run {
                    container,
                    cmd,
                    user,
                    working_dir,
                    env: if env.is_empty() { None } else { Some(env) },
                    privileged,
                },
                allow_write,
            ),
            DockerCommand::Restart {
                container,
                timeout_secs,
                allow_write,
            } => (
                DockerAction::Restart {
                    container,
                    timeout_secs,
                },
                allow_write,
            ),
        };

        let req = DockerRequest {
            action,
            docker_host: docker_host.unwrap_or_else(|| "unix:///var/run/docker.sock".to_string()),
            unix_socket,
            allow_write,
            timeout_secs: cli.timeout,
        };
        let result = DockerOrchestrator::execute(req, tunnel_config).await?;
        Self::print_warnings(&result);
        println!("{}", CliFormatter::format(&result));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_rabbitmq(
        cli: &Cli,
        host: Option<String>,
        scheme: Option<String>,
        port: Option<u16>,
        user: Option<String>,
        password: Option<String>,
        insecure: bool,
        action: RabbitmqCommand,
    ) -> Result<()> {
        let host = host.ok_or_else(|| Error::Config("rabbitmq --host is required".into()))?;
        let scheme = scheme.unwrap_or_else(|| "http".to_string());
        let port = port.unwrap_or_else(|| default_port_for(&scheme));
        let user = user.unwrap_or_else(|| "guest".to_string());
        let password = password.unwrap_or_else(|| "guest".to_string());

        let action = match action {
            RabbitmqCommand::ListQueues {
                vhost,
                name_pattern,
                limit,
            } => RabbitmqAction::ListQueues {
                vhost,
                name_pattern,
                limit,
            },
            RabbitmqCommand::QueueInfo { vhost, name } => RabbitmqAction::QueueInfo { vhost, name },
            RabbitmqCommand::GetMessages {
                vhost,
                queue,
                count,
                truncate_bytes,
            } => RabbitmqAction::GetMessages {
                vhost,
                queue,
                count,
                truncate_bytes,
            },
            RabbitmqCommand::ListBindings {
                vhost,
                source,
                destination,
            } => RabbitmqAction::ListBindings {
                vhost,
                source,
                destination,
            },
            RabbitmqCommand::Overview => RabbitmqAction::Overview,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let req = RabbitmqRequest {
            action,
            scheme,
            host,
            port,
            user,
            password,
            insecure,
            timeout_secs: cli.timeout,
            max_timeout_secs: None,
        };
        let result = RabbitmqOrchestrator::execute(req, tunnel_config).await?;
        Self::print_warnings(&result);
        println!("{}", CliFormatter::format(&result));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_tunnel_serve(
        kind: TunnelServeType,
        listen: SocketAddr,
        ssh_jump: String,
        ssh_user: String,
        ssh_password: Option<String>,
        ssh_key_path: Option<PathBuf>,
        ssh_port: u16,
        target_host: Option<String>,
        target_port: Option<u16>,
        remote_socket: Option<String>,
    ) -> Result<()> {
        let jumps: Vec<String> = ssh_jump
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if jumps.is_empty() {
            return Err(Error::Config(
                "--ssh-jump must not be empty (single host or comma-separated chain)".to_string(),
            ));
        }

        // Validate per-type required + rejected fields.
        match kind {
            TunnelServeType::SshTcp => {
                if remote_socket.is_some() {
                    return Err(Error::Config(
                        "--remote-socket is only valid with --type=ssh-streamlocal".to_string(),
                    ));
                }
                if target_host.is_none() || target_port.is_none() {
                    return Err(Error::Config(
                        "--type=ssh-tcp requires --target-host and --target-port".to_string(),
                    ));
                }
            }
            TunnelServeType::SshStreamlocal => {
                if target_host.is_some() || target_port.is_some() {
                    return Err(Error::Config(
                        "--target-host/--target-port are only valid with --type=ssh-tcp"
                            .to_string(),
                    ));
                }
                if remote_socket.is_none() {
                    return Err(Error::Config(
                        "--type=ssh-streamlocal requires --remote-socket".to_string(),
                    ));
                }
            }
            TunnelServeType::SshSocks => {
                if target_host.is_some() || target_port.is_some() || remote_socket.is_some() {
                    return Err(Error::Config(
                        "--type=ssh-socks doesn't take --target-host/--target-port/--remote-socket"
                            .to_string(),
                    ));
                }
            }
        }

        // Build the right tunnel impl and establish.
        let mut tunnel: Box<dyn Tunnel> = match kind {
            TunnelServeType::SshTcp => {
                let t = SshTunnel::new(
                    jumps,
                    ssh_user,
                    ssh_password,
                    ssh_key_path,
                    ssh_port,
                    target_host.unwrap(),
                    target_port.unwrap(),
                )?
                .with_listen_addr(listen);
                Box::new(t)
            }
            TunnelServeType::SshStreamlocal => {
                let t = StreamLocalTunnel::new(
                    jumps,
                    ssh_user,
                    ssh_password,
                    ssh_key_path,
                    ssh_port,
                    remote_socket.unwrap(),
                )?
                .with_listen_addr(listen);
                Box::new(t)
            }
            TunnelServeType::SshSocks => {
                let t = SocksTunnel::new(jumps, ssh_user, ssh_password, ssh_key_path, ssh_port)?
                    .with_listen_addr(listen);
                Box::new(t)
            }
        };

        let ep = tunnel.establish().await?;
        let shape = match kind {
            TunnelServeType::SshTcp => "ssh-tcp",
            TunnelServeType::SshStreamlocal => "ssh-streamlocal",
            TunnelServeType::SshSocks => "ssh-socks",
        };
        eprintln!(
            "tunnel-serve [{shape}] listening on {host}:{port} (Ctrl-C to stop)",
            host = ep.host,
            port = ep.port,
        );

        // Wait for SIGINT (Ctrl-C) or SIGTERM.
        wait_for_shutdown_signal().await;

        eprintln!("tunnel-serve: shutting down");
        let _ = tunnel.close().await;
        Ok(())
    }
}

/// Block until SIGINT or SIGTERM. On non-unix, falls back to ctrl-c only.
async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Convert repeated `--filter key=value` CLI args into the
/// `HashMap<String, Vec<String>>` shape Docker's filters take.
fn parse_kv_filters(raw: &[String]) -> Result<Option<HashMap<String, Vec<String>>>> {
    if raw.is_empty() {
        return Ok(None);
    }
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for kv in raw {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| Error::Config(format!("filter '{kv}' must be KEY=VALUE")))?;
        map.entry(k.to_string()).or_default().push(v.to_string());
    }
    Ok(Some(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_handler_new() {
        let _handler = CliHandler;
    }
}
