//! PostgreSQL orchestrator: typed request → DB-level read-only-aware
//! `tools4a_pgsql::execute`.

use crate::execute as pgsql_execute;
use crate::execute::PgsqlParams;
use async_trait::async_trait;
use tools4a_core::config::Config;
use tools4a_core::readonly::is_readonly_sql;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig, build_tunnel};

#[derive(Debug, Clone)]
pub struct PgsqlRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub query: String,
    pub allow_write: bool,
}

impl PgsqlRequest {
    pub fn from_config(config: Config, query: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("Pgsql host is required".to_string()))?;
        let port = config.port.unwrap_or(5432);
        let user = config
            .user
            .ok_or_else(|| Error::Config("Pgsql user is required".to_string()))?;

        Ok(PgsqlRequest {
            host,
            port,
            user,
            password: config.password,
            database: config.database,
            query,
            allow_write: false,
        })
    }
}

pub struct PgsqlOrchestrator;

#[async_trait]
impl Service for PgsqlOrchestrator {
    type Request = PgsqlRequest;

    async fn execute(
        req: PgsqlRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        if !req.allow_write && !is_readonly_sql(&req.query) {
            return Err(Error::Service(
                "write operation not allowed without --allow-write \
                 (CLI) / allow_write=true (MCP)"
                    .to_string(),
            ));
        }

        let tunnel = build_tunnel(req.host.clone(), req.port, tunnel_config)?;

        let params = PgsqlParams {
            user: req.user,
            password: req.password,
            database: req.database,
        };

        pgsql_execute(tunnel, params, &req.query, !req.allow_write).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_errors_on_missing_host() {
        let config = Config {
            user: Some("u".to_string()),
            ..Default::default()
        };
        let err = PgsqlRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("host")));
    }

    #[test]
    fn from_config_defaults_port() {
        let config = Config {
            host: Some("h".to_string()),
            user: Some("u".to_string()),
            ..Default::default()
        };
        let req = PgsqlRequest::from_config(config, "SELECT 1".to_string()).unwrap();
        assert_eq!(req.port, 5432);
        assert!(!req.allow_write);
    }

    #[tokio::test]
    async fn execute_rejects_write_without_allow_write() {
        let req = PgsqlRequest {
            host: "127.0.0.1".to_string(),
            port: 5432,
            user: "u".to_string(),
            password: None,
            database: None,
            query: "DELETE FROM t".to_string(),
            allow_write: false,
        };
        let err = PgsqlOrchestrator::execute(req, None).await.unwrap_err();
        assert!(matches!(err, Error::Service(ref msg) if msg.contains("--allow-write")));
    }
}
