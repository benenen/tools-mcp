//! Orchestrator: take a fully-resolved Config, build the right tunnel,
//! call into tools_mcp_mysql::execute. CLI handler and MCP tool both
//! delegate here so teardown semantics are identical.

use crate::config::{Config, TunnelConfig};
use crate::tunnel::{DirectTunnel, SshTunnel};
use tools_mcp_core::{Error, ExecutionResult, Result, Tunnel};
use tools_mcp_mysql::{execute as mysql_execute, MysqlParams};

pub async fn execute(config: Config, query: &str) -> Result<ExecutionResult> {
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

    let params = MysqlParams {
        user,
        password: config.password,
        database: config.database,
    };

    mysql_execute(tunnel, params, query).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_errors_on_missing_host() {
        let config = Config {
            user: Some("root".to_string()),
            ..Default::default()
        };
        let err = execute(config, "SELECT 1").await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_missing_user() {
        let config = Config {
            host: Some("localhost".to_string()),
            ..Default::default()
        };
        let err = execute(config, "SELECT 1").await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("user")));
    }
}
