//! ClickHouse orchestrator: typed request → server-side read-only-aware
//! `tools4a_clickhouse::execute`.

use crate::execute as clickhouse_execute;
use crate::execute::ClickhouseParams;
use async_trait::async_trait;
use tools4a_core::config::Config;
use tools4a_core::readonly::is_readonly_sql;
use tools4a_core::{
    Error, ExecutionResult, Result, Service, TunnelConfig, apply_with_timeout, build_tunnel,
    resolve_effective_timeout,
};

/// Service default for the per-call execution timeout. Analytic queries
/// can run a bit longer than transactional ones — keep this generous.
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone)]
pub struct ClickhouseRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub query: String,
    pub allow_write: bool,
    pub timeout_secs: Option<u64>,
    pub max_timeout_secs: Option<u64>,
}

impl ClickhouseRequest {
    pub fn from_config(config: Config, query: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("Clickhouse host is required".to_string()))?;
        let port = config.port.unwrap_or(8123);
        // ClickHouse ships with a built-in `default` user (no password) —
        // matching that as the implicit fallback keeps the simplest
        // local-dev case (`tools4a clickhouse "SELECT 1" --host=localhost`)
        // working without extra flags.
        let user = config.user.unwrap_or_else(|| "default".to_string());

        Ok(ClickhouseRequest {
            host,
            port,
            user,
            password: config.password,
            database: config.database,
            query,
            allow_write: false,
            timeout_secs: config.timeout_secs,
            max_timeout_secs: None,
        })
    }
}

pub struct ClickhouseOrchestrator;

#[async_trait]
impl Service for ClickhouseOrchestrator {
    type Request = ClickhouseRequest;

    async fn execute(
        req: ClickhouseRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        if !req.allow_write && !is_readonly_sql(&req.query) {
            return Err(Error::Service(
                "write operation not allowed without --allow-write \
                 (CLI) / allow_write=true (MCP)"
                    .to_string(),
            ));
        }

        let deadline =
            resolve_effective_timeout(req.timeout_secs, DEFAULT_TIMEOUT_SECS, req.max_timeout_secs);

        let tunnel = build_tunnel(req.host.clone(), req.port, tunnel_config)?;

        let params = ClickhouseParams {
            user: req.user,
            password: req.password,
            database: req.database,
        };

        let read_only = !req.allow_write;
        let mut result = apply_with_timeout(
            deadline,
            clickhouse_execute(tunnel, params, &req.query, read_only),
        )
        .await?;
        if let Some(w) = deadline.clamp_warning() {
            result.push_warning(w);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_errors_on_missing_host() {
        let config = Config::default();
        let err = ClickhouseRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("host")));
    }

    #[test]
    fn from_config_defaults() {
        let config = Config {
            host: Some("h".to_string()),
            ..Default::default()
        };
        let req = ClickhouseRequest::from_config(config, "SELECT 1".to_string()).unwrap();
        assert_eq!(req.host, "h");
        assert_eq!(req.port, 8123);
        assert_eq!(req.user, "default");
        assert!(!req.allow_write);
    }

    #[test]
    fn from_config_overrides() {
        let config = Config {
            host: Some("ch.example.com".to_string()),
            port: Some(9000),
            user: Some("alice".to_string()),
            password: Some("p".to_string()),
            database: Some("metrics".to_string()),
            ..Default::default()
        };
        let req = ClickhouseRequest::from_config(config, "SELECT 1".to_string()).unwrap();
        assert_eq!(req.host, "ch.example.com");
        assert_eq!(req.port, 9000);
        assert_eq!(req.user, "alice");
        assert_eq!(req.password.as_deref(), Some("p"));
        assert_eq!(req.database.as_deref(), Some("metrics"));
    }

    #[tokio::test]
    async fn execute_rejects_write_without_allow_write() {
        let req = ClickhouseRequest {
            host: "127.0.0.1".to_string(),
            port: 8123,
            user: "default".to_string(),
            password: None,
            database: None,
            query: "INSERT INTO t VALUES (1)".to_string(),
            allow_write: false,
            timeout_secs: None,
            max_timeout_secs: None,
        };
        let err = ClickhouseOrchestrator::execute(req, None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Service(ref msg) if msg.contains("--allow-write")));
    }
}
