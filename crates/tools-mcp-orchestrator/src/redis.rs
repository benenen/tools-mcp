//! Redis orchestrator: typed request → `tools_mcp_redis::execute` with
//! the right tunnel built from the request's tunnel config.

use crate::config::Config;
use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools_mcp_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools_mcp_redis::{RedisParams, execute as redis_execute};

/// Typed Redis request. Caller (CLI handler / MCP tool) resolves
/// Profile/YAML/CLI args into this struct before dispatching.
#[derive(Debug, Clone)]
pub struct RedisRequest {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub db: u32,
    pub command: String,
}

impl RedisRequest {
    /// Build a typed RedisRequest by validating + draining a Config.
    ///
    /// `config.host` is required (returns `Error::Config` if missing).
    /// `config.port` defaults to 6379. `config.db` defaults to 0.
    pub fn from_config(config: Config, command: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("Redis host is required".to_string()))?;
        let port = config.port.unwrap_or(6379);
        let db = config.db.unwrap_or(0);

        Ok(RedisRequest {
            host,
            port,
            password: config.password,
            db,
            command,
        })
    }
}

pub struct RedisOrchestrator;

#[async_trait]
impl Service for RedisOrchestrator {
    type Request = RedisRequest;

    async fn execute(
        req: RedisRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        let tunnel: Box<dyn Tunnel> = match tunnel_config {
            None | Some(TunnelConfig::Direct) => {
                Box::new(DirectTunnel::new(req.host.clone(), req.port))
            }
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
                    req.host.clone(),
                    req.port,
                )?)
            }
        };

        let params = RedisParams {
            password: req.password,
            db: req.db,
        };

        redis_execute(tunnel, params, &req.command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_config_errors_on_missing_host() {
        let config = Config::default();
        let err = RedisRequest::from_config(config, "PING".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }

    #[test]
    fn test_from_config_succeeds_with_required_fields() {
        let config = Config {
            host: Some("localhost".to_string()),
            password: Some("pwd".to_string()),
            db: Some(2),
            port: Some(6380),
            ..Default::default()
        };
        let req = RedisRequest::from_config(config, "PING".to_string()).unwrap();
        assert_eq!(req.host, "localhost");
        assert_eq!(req.port, 6380);
        assert_eq!(req.password.as_deref(), Some("pwd"));
        assert_eq!(req.db, 2);
        assert_eq!(req.command, "PING");
    }

    #[test]
    fn test_from_config_defaults_port_and_db() {
        let config = Config {
            host: Some("h".to_string()),
            ..Default::default()
        };
        let req = RedisRequest::from_config(config, "PING".to_string()).unwrap();
        assert_eq!(req.port, 6379);
        assert_eq!(req.db, 0);
    }
}
