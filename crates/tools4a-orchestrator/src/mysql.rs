//! MySQL orchestrator: typed request → `tools4a_mysql::execute` with
//! the right tunnel built from the request's tunnel config.

use crate::readonly::is_readonly_sql;
use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools4a_mysql::{MysqlParams, execute as mysql_execute};

/// Typed MySQL request. Caller (CLI handler / MCP tool) resolves
/// Profile/YAML/CLI args into this struct before dispatching.
#[derive(Debug, Clone)]
pub struct MysqlRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub query: String,
    /// When false (default), only read-only queries are allowed; writes
    /// are rejected before connecting AND the session is forced into
    /// `TRANSACTION READ ONLY` mode as a second line of defense.
    pub allow_write: bool,
}

impl MysqlRequest {
    /// Build a typed MysqlRequest by validating + draining a Config.
    ///
    /// `config.host` and `config.user` are required (returns
    /// `Error::Config` if missing). `config.port` defaults to 3306.
    /// `allow_write` defaults to false at this layer — callers must opt
    /// in to write operations by setting it to true on the returned
    /// struct or via an explicit constructor variant.
    pub fn from_config(config: crate::config::Config, query: String) -> Result<Self> {
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
    use crate::config::Config;

    #[test]
    fn test_from_config_errors_on_missing_host() {
        let config = Config {
            user: Some("root".to_string()),
            ..Default::default()
        };
        let err = MysqlRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }

    #[test]
    fn test_from_config_errors_on_missing_user() {
        let config = Config {
            host: Some("localhost".to_string()),
            ..Default::default()
        };
        let err = MysqlRequest::from_config(config, "SELECT 1".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("user")));
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
        assert_eq!(req.user, "root");
        assert_eq!(req.password.as_deref(), Some("pwd"));
        assert_eq!(req.database.as_deref(), Some("mydb"));
        assert_eq!(req.query, "SELECT 1");
        assert!(!req.allow_write);
    }

    #[test]
    fn test_from_config_defaults_port_to_3306() {
        let config = Config {
            host: Some("h".to_string()),
            user: Some("u".to_string()),
            ..Default::default()
        };
        let req = MysqlRequest::from_config(config, "SELECT 1".to_string()).unwrap();
        assert_eq!(req.port, 3306);
    }

    #[tokio::test]
    async fn test_execute_rejects_write_without_allow_write() {
        // No live MySQL server needed — the readonly check happens before
        // any connection attempt.
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
        assert!(
            matches!(err, Error::Service(ref msg) if msg.contains("--allow-write")),
            "expected --allow-write hint in error, got {err:?}"
        );
    }
}
