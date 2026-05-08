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
#[command(name = "tools-mcp")]
#[command(about = "Unified tool for SSH, MySQL, Redis connections with MCP support")]
#[command(override_usage = "tools-mcp [GLOBAL OPTIONS] [COMMAND]")]
#[command(after_help = USAGE_LEGEND)]
pub struct Cli {
    /// Path to YAML config file
    #[arg(long, global = true, help_heading = "Global Options")]
    pub config: Option<PathBuf>,

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
    #[command(override_usage = "tools-mcp [GLOBAL OPTIONS] mysql [OPTIONS] <QUERY>")]
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
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_parse_mysql_command() {
        let args = Cli::try_parse_from(&[
            "tools-mcp",
            "mysql",
            "SELECT 1",
            "--host=localhost",
            "--user=root",
        ]).unwrap();

        match args.command {
            Some(Commands::Mysql { query, host, user, .. }) => {
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
        let result = Cli::try_parse_from(&[
            "tools-mcp",
            "--ssh-jump=bastion.com",
            "mysql",
            "SELECT 1",
        ]);
        assert!(result.is_err(), "expected parse error when --ssh-jump used without --tunnel");
    }

    #[test]
    fn test_tunnel_kind_parse() {
        let cli = Cli::try_parse_from(&[
            "tools-mcp",
            "--tunnel=ssh",
            "mysql",
            "SELECT 1",
        ]).unwrap();
        assert!(matches!(cli.tunnel, Some(TunnelKind::Ssh)));

        let cli = Cli::try_parse_from(&[
            "tools-mcp",
            "--tunnel=direct",
            "mysql",
            "SELECT 1",
        ]).unwrap();
        assert!(matches!(cli.tunnel, Some(TunnelKind::Direct)));
    }
}
