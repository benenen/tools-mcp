//! MySQL orchestrator: typed request → DB-level read-only-aware
//! `tools4a_mysql::execute`. Lives in this leaf crate (not the deleted
//! `tools4a-orchestrator`) so the MCP impl below can dispatch through
//! it without a dep cycle.

use crate::execute as mysql_execute;
use crate::execute::MysqlParams;
use async_trait::async_trait;
use tools4a_core::config::Config;
use tools4a_core::readonly::is_readonly_sql;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig, build_tunnel};

#[derive(Debug, Clone)]
pub struct MysqlRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub query: String,
    pub allow_write: bool,
}

impl MysqlRequest {
    pub fn from_config(config: Config, query: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("MySQL host is required".to_string()))?;
        let port = config.port.unwrap_or(3306);
        let user = config
            .user
            .ok_or_else(|| Error::Config("MySQL user is required".to_string()))?;

        Ok(MysqlRequest {
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

pub struct MysqlOrchestrator;

#[async_trait]
impl Service for MysqlOrchestrator {
    type Request = MysqlRequest;

    async fn execute(
        req: MysqlRequest,
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

        let params = MysqlParams {
            user: req.user,
            password: req.password,
            database: req.database,
        };

        mysql_execute(tunnel, params, &req.query, !req.allow_write).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_config_errors_on_missing_host() {
        let config = Config {
            user: Some("root".to_string()),
            ..Default::default()
        };
        let err = MysqlRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("host")));
    }

    #[test]
    fn test_from_config_errors_on_missing_user() {
        let config = Config {
            host: Some("localhost".to_string()),
            ..Default::default()
        };
        let err = MysqlRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("user")));
    }

    #[test]
    fn test_from_config_succeeds_with_required_fields() {
        let config = Config {
            host: Some("localhost".to_string()),
            user: Some("root".to_string()),
            password: Some("pwd".to_string()),
            database: Some("mydb".to_string()),
            port: Some(3307),
            ..Default::default()
        };
        let req = MysqlRequest::from_config(config, "SELECT 1".to_string()).unwrap();
        assert_eq!(req.host, "localhost");
        assert_eq!(req.port, 3307);
        assert!(!req.allow_write);
    }

    #[tokio::test]
    async fn test_execute_rejects_write_without_allow_write() {
        let req = MysqlRequest {
            host: "127.0.0.1".to_string(),
            port: 3306,
            user: "u".to_string(),
            password: None,
            database: None,
            query: "INSERT INTO t VALUES (1)".to_string(),
            allow_write: false,
        };
        let err = MysqlOrchestrator::execute(req, None).await.unwrap_err();
        assert!(matches!(err, Error::Service(ref msg) if msg.contains("--allow-write")));
    }
}
