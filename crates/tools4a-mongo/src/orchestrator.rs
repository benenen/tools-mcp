//! Mongo orchestrator: typed request → `tools4a_mongo::execute`.

use crate::execute as mongo_execute;
use crate::execute::MongoParams;
use async_trait::async_trait;
use tools4a_core::config::Config;
use tools4a_core::readonly::is_readonly_mongo;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig};
use tools4a_tunnel::build_tunnel;

#[derive(Debug, Clone)]
pub struct MongoRequest {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: String,
    pub command: String,
    pub allow_write: bool,
}

impl MongoRequest {
    pub fn from_config(config: Config, command: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("Mongo host is required".to_string()))?;
        let port = config.port.unwrap_or(27017);
        let database = config
            .database
            .ok_or_else(|| Error::Config("Mongo database is required".to_string()))?;

        Ok(MongoRequest {
            host,
            port,
            user: config.user,
            password: config.password,
            database,
            command,
            allow_write: false,
        })
    }
}

pub struct MongoOrchestrator;

#[async_trait]
impl Service for MongoOrchestrator {
    type Request = MongoRequest;

    async fn execute(
        req: MongoRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        if !req.allow_write && !is_readonly_mongo(&req.command) {
            return Err(Error::Service(
                "write operation not allowed without --allow-write \
                 (CLI) / allow_write=true (MCP)"
                    .to_string(),
            ));
        }

        let tunnel = build_tunnel(req.host.clone(), req.port, tunnel_config)?;

        let params = MongoParams {
            user: req.user,
            password: req.password,
            database: req.database,
        };

        mongo_execute(tunnel, params, &req.command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_errors_on_missing_host() {
        let config = Config {
            database: Some("d".to_string()),
            ..Default::default()
        };
        let err = MongoRequest::from_config(config, "{}".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("host")));
    }

    #[test]
    fn from_config_errors_on_missing_database() {
        let config = Config {
            host: Some("h".to_string()),
            ..Default::default()
        };
        let err = MongoRequest::from_config(config, "{}".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("database")));
    }

    #[tokio::test]
    async fn execute_rejects_write_without_allow_write() {
        let req = MongoRequest {
            host: "127.0.0.1".to_string(),
            port: 27017,
            user: None,
            password: None,
            database: "test".to_string(),
            command: r#"{"insert":"t","documents":[{"x":1}]}"#.to_string(),
            allow_write: false,
        };
        let err = MongoOrchestrator::execute(req, None).await.unwrap_err();
        assert!(matches!(err, Error::Service(ref msg) if msg.contains("--allow-write")));
    }
}
