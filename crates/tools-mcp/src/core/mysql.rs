use crate::config::{Config, TunnelConfig};
use crate::connection::{Connection, MySQLConnection};
use tools_mcp_core::{Error, Result};
use crate::executor::MySQLExecutor;
use crate::output::ExecutionResult;
use crate::tunnel::{DirectTunnel, SshTunnel, Tunnel};

/// Execute a single MySQL query against the connection described by `config`.
///
/// Errors if `config.host` or `config.user` is missing. Always tears down
/// the underlying connection (and SSH tunnel, if any) before returning,
/// regardless of whether the query succeeded.
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

    let mut conn = MySQLConnection::new(tunnel, user, config.password, config.database);
    let exec_result = MySQLExecutor::execute(&mut conn, query).await;
    let _ = conn.disconnect().await;
    exec_result
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
