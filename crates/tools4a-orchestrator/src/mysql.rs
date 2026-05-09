//! MySQL orchestrator: typed request → `tools4a_mysql::execute` with
//! the right tunnel built from the request's tunnel config.

use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools4a_core::{ExecutionResult, Result, Service, Tunnel, TunnelConfig};
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
}

impl MysqlRequest {
    /// Build a typed MysqlRequest by validating + draining a Config.
    ///
    /// `config.host` and `config.user` are required (returns
    /// `Error::Config` if missing). `config.port` defaults to 3306.
    pub fn from_config(config: crate::config::Config, query: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| tools4a_core::Error::Config("MySQL host is required".to_string()))?;
        let port = config.port.unwrap_or(3306);
        let user = config
            .user
            .ok_or_else(|| tools4a_core::Error::Config("MySQL user is required".to_string()))?;

        Ok(MysqlRequest {
            host,
            port,
            user,
            password: config.password,
            database: config.database,
            query,
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

        mysql_execute(tunnel, params, &req.query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tools4a_core::Error;

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
}
