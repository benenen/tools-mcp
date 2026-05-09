//! Pgsql orchestrator: typed request → `tools4a_pgsql::execute` with
//! the right tunnel built from the request's tunnel config.

use crate::config::Config;
use crate::readonly::is_readonly_sql;
use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools4a_pgsql::{PgsqlParams, execute as pgsql_execute};

/// Typed PostgreSQL request. Caller (CLI handler / MCP tool) resolves
/// Profile/YAML/CLI args into this struct before dispatching.
#[derive(Debug, Clone)]
pub struct PgsqlRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub query: String,
    /// When false (default), only read-only queries are allowed; the
    /// session also runs `SET default_transaction_read_only = on` as a
    /// second line of defense.
    pub allow_write: bool,
}

impl PgsqlRequest {
    /// Build a typed PgsqlRequest by validating + draining a Config.
    ///
    /// `config.host` and `config.user` are required (returns
    /// `Error::Config` if missing). `config.port` defaults to 5432.
    /// `allow_write` defaults to false; callers opt in explicitly.
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
    fn test_from_config_errors_on_missing_host() {
        let config = Config {
            user: Some("u".to_string()),
            ..Default::default()
        };
        let err = PgsqlRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }

    #[test]
    fn test_from_config_errors_on_missing_user() {
        let config = Config {
            host: Some("h".to_string()),
            ..Default::default()
        };
        let err = PgsqlRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("user")));
    }

    #[test]
    fn test_from_config_succeeds_with_required_fields() {
        let config = Config {
            host: Some("h".to_string()),
            user: Some("u".to_string()),
            password: Some("p".to_string()),
            database: Some("d".to_string()),
            port: Some(5433),
            ..Default::default()
        };
        let req = PgsqlRequest::from_config(config, "SELECT 1".to_string()).unwrap();
        assert_eq!(req.host, "h");
        assert_eq!(req.port, 5433);
        assert_eq!(req.user, "u");
        assert_eq!(req.password.as_deref(), Some("p"));
        assert_eq!(req.database.as_deref(), Some("d"));
        assert_eq!(req.query, "SELECT 1");
    }

    #[test]
    fn test_from_config_defaults_port() {
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
    async fn test_execute_rejects_write_without_allow_write() {
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
        assert!(
            matches!(err, Error::Service(ref msg) if msg.contains("--allow-write")),
            "expected --allow-write hint, got {err:?}"
        );
    }
}
