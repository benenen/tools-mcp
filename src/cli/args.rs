use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "tools-mcp")]
#[command(about = "Unified tool for SSH, MySQL, Redis connections with MCP support")]
pub struct Cli {
    /// Path to YAML config file
    #[arg(long, global = true)]
    pub config: Option<String>,

    /// Tunnel type (direct or ssh)
    #[arg(long, global = true, value_enum)]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host (used when --tunnel=ssh)
    #[arg(long, global = true)]
    pub ssh_jump: Option<String>,

    /// SSH jump user (used when --tunnel=ssh)
    #[arg(long, global = true)]
    pub ssh_user: Option<String>,

    /// SSH jump password (used when --tunnel=ssh)
    #[arg(long, global = true)]
    pub ssh_password: Option<String>,

    /// SSH key path (used when --tunnel=ssh)
    #[arg(long, global = true)]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (used when --tunnel=ssh)
    #[arg(long, global = true)]
    pub ssh_port: Option<u16>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lowercase")]
pub enum TunnelKind {
    Direct,
    Ssh,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute a MySQL query
    Mysql {
        /// SQL query to execute
        query: String,

        /// MySQL host
        #[arg(long)]
        host: Option<String>,

        /// MySQL port
        #[arg(long)]
        port: Option<u16>,

        /// MySQL user
        #[arg(long)]
        user: Option<String>,

        /// MySQL password
        #[arg(long)]
        password: Option<String>,

        /// Database name
        #[arg(long)]
        database: Option<String>,

        /// Profile name from config
        #[arg(long)]
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
}
