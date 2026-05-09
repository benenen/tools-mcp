//! Redis orchestrator: typed request → `tools4a_redis::execute`.

use crate::execute as redis_execute;
use crate::execute::RedisParams;
use async_trait::async_trait;
use tools4a_core::config::Config;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig, build_tunnel};

#[derive(Debug, Clone)]
pub struct RedisRequest {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub db: u32,
    pub command: String,
}

impl RedisRequest {
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
        let tunnel = build_tunnel(req.host.clone(), req.port, tunnel_config)?;

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
    fn from_config_errors_on_missing_host() {
        let err = RedisRequest::from_config(Config::default(), "PING".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("host")));
    }

    #[test]
    fn from_config_defaults_port_and_db() {
        let config = Config {
            host: Some("h".to_string()),
            ..Default::default()
        };
        let req = RedisRequest::from_config(config, "PING".to_string()).unwrap();
        assert_eq!(req.port, 6379);
        assert_eq!(req.db, 0);
    }
}
