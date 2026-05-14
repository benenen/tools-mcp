use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

// Usage 行里的 `[GLOBAL OPTIONS]` 是手写占位符（非 clap 内建），覆盖所有
// `global = true` 的参数；`[OPTIONS]` 是 clap 内建占位符，表示子命令自己的
// 参数。下方的 `USAGE_LEGEND`（通过 `after_help`）会把这两个占位符对应
// 到帮助正文里的 section 名，让 CLI 用户一眼就能看清"哪些放命令前、哪些
// 放命令后"。新增全局参数时只需要给它打 `global = true` + 合适的
// `help_heading`，`override_usage` 字符串不必改动。

/// 帮助底部的占位符图例，root 和子命令共用。
const USAGE_LEGEND: &str = "\
Placeholder legend:
  [GLOBAL OPTIONS]  Tool-wide flags. Shown below under \"Global Options\"
                    (--config) and \"Tunnel\" (--tunnel, --ssh-*) sections.
                    Can be placed before OR after the subcommand.
  [OPTIONS]         Subcommand-specific flags. Shown under the subcommand's
                    own section (e.g. \"MySQL\" for the mysql subcommand).";

#[derive(Parser, Debug)]
#[command(name = "tools4a")]
#[command(about = "Unified tool for SSH, MySQL, Redis connections with MCP support")]
#[command(override_usage = "tools4a [GLOBAL OPTIONS] [COMMAND]")]
#[command(after_help = USAGE_LEGEND)]
pub struct Cli {
    /// Path to YAML config file
    #[arg(long, global = true, help_heading = "Global Options")]
    pub config: Option<PathBuf>,

    /// Per-call execution timeout in seconds. Capped by
    /// TOOLS4A_MAX_TIMEOUT_SECS or `[defaults] max_timeout_secs` in
    /// ~/.config/tools4a/config.toml. When unset, each service applies
    /// its own default (SQL: 30s, Redis: 10s, HTTP: 60s, SSH: 300s).
    #[arg(
        long,
        global = true,
        value_name = "SECS",
        help_heading = "Global Options"
    )]
    pub timeout: Option<u64>,

    /// Tunnel type (direct or ssh)
    #[arg(long, global = true, value_enum, help_heading = "Tunnel")]
    pub tunnel: Option<TunnelKind>,

    #[command(flatten)]
    pub ssh: SshTunnelArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Args, Debug, Clone)]
pub struct SshTunnelArgs {
    /// SSH jump host (used when --tunnel=ssh)
    #[arg(long, global = true, requires = "tunnel", help_heading = "Tunnel")]
    pub ssh_jump: Option<String>,

    /// SSH jump user (used when --tunnel=ssh)
    #[arg(long, global = true, requires = "tunnel", help_heading = "Tunnel")]
    pub ssh_user: Option<String>,

    /// SSH jump password (used when --tunnel=ssh)
    #[arg(long, global = true, requires = "tunnel", help_heading = "Tunnel")]
    pub ssh_password: Option<String>,

    /// SSH key path (used when --tunnel=ssh)
    #[arg(long, global = true, requires = "tunnel", help_heading = "Tunnel")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (used when --tunnel=ssh)
    #[arg(long, global = true, requires = "tunnel", help_heading = "Tunnel")]
    pub ssh_port: Option<u16>,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum TunnelKind {
    Direct,
    Ssh,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Execute a MySQL query
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] mysql [OPTIONS] <QUERY>")]
    #[command(after_help = USAGE_LEGEND)]
    Mysql {
        /// SQL query to execute
        query: String,

        /// MySQL host
        #[arg(long, help_heading = "MySQL")]
        host: Option<String>,

        /// MySQL port
        #[arg(long, help_heading = "MySQL")]
        port: Option<u16>,

        /// MySQL user
        #[arg(long, help_heading = "MySQL")]
        user: Option<String>,

        /// MySQL password
        #[arg(long, help_heading = "MySQL")]
        password: Option<String>,

        /// Database name
        #[arg(long, help_heading = "MySQL")]
        database: Option<String>,

        /// Profile name from config
        #[arg(long, help_heading = "MySQL")]
        profile: Option<String>,

        /// Allow write operations (INSERT/UPDATE/DELETE/DDL). Off by
        /// default; the session also runs in TRANSACTION READ ONLY mode
        /// unless this is set.
        #[arg(long = "allow-write", help_heading = "MySQL")]
        allow_write: bool,
    },

    /// Execute a PostgreSQL query
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] pgsql [OPTIONS] <QUERY>")]
    #[command(after_help = USAGE_LEGEND)]
    Pgsql {
        /// SQL query to execute
        query: String,

        /// Pgsql host
        #[arg(long, help_heading = "Pgsql")]
        host: Option<String>,

        /// Pgsql port (default 5432)
        #[arg(long, help_heading = "Pgsql")]
        port: Option<u16>,

        /// Pgsql user
        #[arg(long, help_heading = "Pgsql")]
        user: Option<String>,

        /// Pgsql password
        #[arg(long, help_heading = "Pgsql")]
        password: Option<String>,

        /// Database name
        #[arg(long, help_heading = "Pgsql")]
        database: Option<String>,

        /// Profile name from config
        #[arg(long, help_heading = "Pgsql")]
        profile: Option<String>,

        /// Allow write operations. Off by default; the session also runs
        /// with `default_transaction_read_only = on` unless this is set.
        #[arg(long = "allow-write", help_heading = "Pgsql")]
        allow_write: bool,
    },

    /// Execute a ClickHouse SQL query (over HTTP)
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] clickhouse [OPTIONS] <QUERY>")]
    #[command(after_help = USAGE_LEGEND)]
    Clickhouse {
        /// SQL query to execute
        query: String,

        /// ClickHouse host
        #[arg(long, help_heading = "ClickHouse")]
        host: Option<String>,

        /// ClickHouse HTTP port (default 8123)
        #[arg(long, help_heading = "ClickHouse")]
        port: Option<u16>,

        /// ClickHouse user (default "default")
        #[arg(long, help_heading = "ClickHouse")]
        user: Option<String>,

        /// ClickHouse password
        #[arg(long, help_heading = "ClickHouse")]
        password: Option<String>,

        /// Database name
        #[arg(long, help_heading = "ClickHouse")]
        database: Option<String>,

        /// Profile name from config
        #[arg(long, help_heading = "ClickHouse")]
        profile: Option<String>,

        /// Allow write operations. Off by default; the session also runs
        /// with `readonly=1` server-side unless this is set.
        #[arg(long = "allow-write", help_heading = "ClickHouse")]
        allow_write: bool,
    },

    /// Execute a Redis command
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] redis [OPTIONS] <COMMAND>")]
    #[command(after_help = USAGE_LEGEND)]
    Redis {
        /// Redis command to execute (e.g. "GET key" or "HSET h f1 v1").
        command: String,

        /// Redis host
        #[arg(long, help_heading = "Redis")]
        host: Option<String>,

        /// Redis port (default 6379)
        #[arg(long, help_heading = "Redis")]
        port: Option<u16>,

        /// Redis password
        #[arg(long, help_heading = "Redis")]
        password: Option<String>,

        /// Redis database number (default 0)
        #[arg(long, help_heading = "Redis")]
        db: Option<u32>,

        /// Profile name from config
        #[arg(long, help_heading = "Redis")]
        profile: Option<String>,
    },

    /// Execute a MongoDB command (JSON document passed to db.runCommand)
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] mongo [OPTIONS] <COMMAND>")]
    #[command(after_help = USAGE_LEGEND)]
    Mongo {
        /// Mongo command as a JSON object (e.g. `{"find":"users","filter":{}}`)
        command: String,

        /// Mongo host
        #[arg(long, help_heading = "Mongo")]
        host: Option<String>,

        /// Mongo port (default 27017)
        #[arg(long, help_heading = "Mongo")]
        port: Option<u16>,

        /// Mongo user (optional — Mongo allows unauthenticated connections)
        #[arg(long, help_heading = "Mongo")]
        user: Option<String>,

        /// Mongo password (optional)
        #[arg(long, help_heading = "Mongo")]
        password: Option<String>,

        /// Database name (required for runCommand)
        #[arg(long, help_heading = "Mongo")]
        database: Option<String>,

        /// Profile name from config
        #[arg(long, help_heading = "Mongo")]
        profile: Option<String>,

        /// Allow write commands (insert/update/delete/drop/findAndModify/
        /// aggregate-with-$out etc.). Off by default — Mongo has no
        /// per-session read-only mode, so this is the only guard.
        #[arg(long = "allow-write", help_heading = "Mongo")]
        allow_write: bool,
    },

    /// Execute an HTTP request
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] http [OPTIONS] <METHOD> <URL>")]
    #[command(after_help = USAGE_LEGEND)]
    Http {
        /// HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS).
        method: String,

        /// Full URL (http:// or https://).
        url: String,

        /// Extra header `Name: Value`. Repeat for multiple headers.
        #[arg(long = "header", short = 'H', help_heading = "HTTP")]
        headers: Vec<String>,

        /// Request body (raw string).
        #[arg(long, help_heading = "HTTP", conflicts_with = "data_file")]
        data: Option<String>,

        /// Read request body from a file path.
        #[arg(long = "data-file", help_heading = "HTTP", conflicts_with = "data")]
        data_file: Option<std::path::PathBuf>,

        /// Set Content-Type: application/json (does not transform the body).
        #[arg(long, help_heading = "HTTP")]
        json: bool,

        /// `Authorization: Bearer <TOKEN>` shortcut.
        #[arg(long, help_heading = "HTTP", conflicts_with = "basic")]
        bearer: Option<String>,

        /// HTTP Basic auth as `user:password`.
        #[arg(long, help_heading = "HTTP", conflicts_with = "bearer")]
        basic: Option<String>,

        /// Accept invalid TLS certificates (e.g. self-signed). DANGER: only
        /// use for trusted internal services.
        #[arg(long, help_heading = "HTTP")]
        insecure: bool,

        /// Print full ExecutionResult table (status + headers + body) instead
        /// of just the body. Default: print body only.
        #[arg(long = "include-headers", short = 'i', help_heading = "HTTP")]
        include_headers: bool,
    },

    /// Execute a shell command on an SSH target
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] ssh [OPTIONS] <COMMAND>")]
    #[command(after_help = USAGE_LEGEND)]
    Ssh {
        /// Shell command to execute on the target.
        command: String,

        /// Target SSH host.
        #[arg(long, help_heading = "SSH")]
        host: String,

        /// Target SSH port (default 22).
        #[arg(long, help_heading = "SSH", default_value_t = 22)]
        port: u16,

        /// Target SSH user.
        #[arg(long, help_heading = "SSH")]
        user: String,

        /// Target SSH password (mutually exclusive with --key-path).
        #[arg(long, help_heading = "SSH", conflicts_with = "key_path")]
        password: Option<String>,

        /// Target SSH key path. Unencrypted keys only (passphrases not
        /// supported in this phase).
        #[arg(long = "key-path", help_heading = "SSH", conflicts_with = "password")]
        key_path: Option<std::path::PathBuf>,

        /// Print full ExecutionResult table (exit_code + stdout + stderr)
        /// instead of streaming stdout/stderr to the terminal. Default:
        /// stream stdout to stdout, stderr to stderr, exit with the
        /// remote exit code.
        #[arg(long = "include-headers", short = 'i', help_heading = "SSH")]
        include_headers: bool,
    },

    /// Run an agent-browser CLI subcommand (browser automation)
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] browser [OPTIONS] <SUBCOMMAND> [ARGS]...")]
    #[command(after_help = USAGE_LEGEND)]
    Browser {
        /// agent-browser subcommand (e.g. open, click, snapshot, batch, eval, screenshot).
        subcommand: String,

        /// Arguments passed after <SUBCOMMAND> verbatim to agent-browser.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,

        /// agent-browser --session NAME (isolates daemon state).
        #[arg(long, help_heading = "Browser")]
        session: Option<String>,

        /// agent-browser --proxy URL (e.g. socks5://127.0.0.1:1080).
        #[arg(long, help_heading = "Browser")]
        proxy: Option<String>,

        /// agent-browser --proxy-bypass HOSTS (comma-separated).
        #[arg(long = "proxy-bypass", help_heading = "Browser")]
        proxy_bypass: Option<String>,

        /// agent-browser --args FLAGS — extra Chromium launch arguments.
        #[arg(long = "browser-args", help_heading = "Browser")]
        browser_args: Option<String>,

        /// Override the agent-browser binary path. Defaults to
        /// $AGENT_BROWSER_BIN, then "agent-browser" on $PATH.
        #[arg(long, help_heading = "Browser")]
        bin: Option<std::path::PathBuf>,

        /// Print full ExecutionResult table (exit_code + stdout + stderr)
        /// instead of streaming stdout/stderr to the terminal. Default:
        /// stream stdout to stdout, stderr to stderr, exit with the
        /// agent-browser exit code.
        #[arg(long = "include-headers", short = 'i', help_heading = "Browser")]
        include_headers: bool,
    },

    /// Talk to a Docker daemon (local socket, local/remote TCP, or remote
    /// unix socket via SSH tunnel). Read actions are unrestricted; write
    /// actions (run, restart) require --allow-write.
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] docker [OPTIONS] <SUBCOMMAND> [ARGS]...")]
    #[command(after_help = USAGE_LEGEND)]
    Docker {
        /// Docker daemon endpoint. unix:///path or tcp://host:port.
        /// Default: unix:///var/run/docker.sock.
        #[arg(long = "docker-host", global = true, help_heading = "Docker")]
        docker_host: Option<String>,

        /// Remote unix socket path. Only valid with --tunnel=ssh; uses
        /// StreamLocalTunnel to forward the remote socket through SSH.
        #[arg(long = "unix-socket", global = true, help_heading = "Docker")]
        unix_socket: Option<String>,

        #[command(subcommand)]
        action: DockerCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DockerCommand {
    /// List containers.
    Ps {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        limit: Option<i32>,
        /// Filters in `key=value` form (repeatable). Example: --filter name=app --filter status=running
        #[arg(long = "filter", value_name = "KEY=VALUE")]
        filters: Vec<String>,
    },
    /// Inspect a container (returns full JSON spec).
    Inspect { container: String },
    /// Fetch container logs (one-shot, no follow).
    Logs {
        container: String,
        #[arg(long, default_value = "100")]
        tail: String,
        #[arg(long, default_value_t = true)]
        stdout: bool,
        #[arg(long, default_value_t = true)]
        stderr: bool,
        #[arg(long)]
        timestamps: bool,
        #[arg(long)]
        since: Option<i32>,
    },
    /// One-shot resource stats (cpu/mem/net/io).
    Stats { container: String },
    /// List processes inside a container.
    Top {
        container: String,
        #[arg(long = "ps-args")]
        ps_args: Option<String>,
    },
    /// Run a command inside a container. Requires --allow-write.
    Run {
        container: String,
        /// Command + arguments. e.g. tools4a docker run my-c -- sh -c "jstack 1"
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        cmd: Vec<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long = "working-dir")]
        working_dir: Option<String>,
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,
        #[arg(long)]
        privileged: bool,
        #[arg(long = "allow-write")]
        allow_write: bool,
    },
    /// Restart a container. Requires --allow-write.
    Restart {
        container: String,
        #[arg(long = "timeout-secs")]
        timeout_secs: Option<i32>,
        #[arg(long = "allow-write")]
        allow_write: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_parse_mysql_command() {
        let args = Cli::try_parse_from([
            "tools4a",
            "mysql",
            "SELECT 1",
            "--host=localhost",
            "--user=root",
        ])
        .unwrap();

        match args.command {
            Some(Commands::Mysql {
                query, host, user, ..
            }) => {
                assert_eq!(query, "SELECT 1");
                assert_eq!(host, Some("localhost".to_string()));
                assert_eq!(user, Some("root".to_string()));
            }
            _ => panic!("Expected Mysql command"),
        }
    }

    #[test]
    fn test_ssh_flag_requires_tunnel() {
        // Providing --ssh-jump without --tunnel should fail parsing
        let result =
            Cli::try_parse_from(["tools4a", "--ssh-jump=bastion.com", "mysql", "SELECT 1"]);
        assert!(
            result.is_err(),
            "expected parse error when --ssh-jump used without --tunnel"
        );
    }

    #[test]
    fn test_tunnel_kind_parse() {
        let cli = Cli::try_parse_from(["tools4a", "--tunnel=ssh", "mysql", "SELECT 1"]).unwrap();
        assert!(matches!(cli.tunnel, Some(TunnelKind::Ssh)));

        let cli = Cli::try_parse_from(["tools4a", "--tunnel=direct", "mysql", "SELECT 1"]).unwrap();
        assert!(matches!(cli.tunnel, Some(TunnelKind::Direct)));
    }
}
