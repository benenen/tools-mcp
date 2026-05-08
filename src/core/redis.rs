//! Orchestrator: take a fully-resolved Config, build the right tunnel,
//! call into tools_mcp_redis::execute. CLI handler and MCP tool both
//! delegate here so teardown semantics are identical.

use tools_mcp_core::{Error, ExecutionResult, Result, Tunnel, TunnelConfig};
use tools_mcp_orchestrator::config::Config;
use tools_mcp_orchestrator::tunnel::{DirectTunnel, SshTunnel};
use tools_mcp_redis::{RedisParams, execute as redis_execute};

pub async fn execute(config: Config, command: &str) -> Result<ExecutionResult> {
    let host = config
        .host
        .ok_or_else(|| Error::Config("Redis host is required".to_string()))?;
    let port = config.port.unwrap_or(6379);
    let db = config.db.unwrap_or(0);

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

    let params = RedisParams {
        password: config.password,
        db,
    };

    redis_execute(tunnel, params, command).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_errors_on_missing_host() {
        let config = Config::default();
        let err = execute(config, "PING").await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }
}
