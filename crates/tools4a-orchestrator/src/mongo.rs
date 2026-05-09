//! Mongo orchestrator: typed request → `tools4a_mongo::execute` with
//! the right tunnel built from the request's tunnel config.

use crate::config::Config;
use crate::readonly::is_readonly_mongo;
use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools4a_mongo::{MongoParams, execute as mongo_execute};

/// Typed Mongo request. Caller (CLI handler / MCP tool) resolves
/// Profile/YAML/CLI args into this struct before dispatching.
#[derive(Debug, Clone)]
pub struct MongoRequest {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: String,
    pub command: String,
    /// When false (default), only read-only commands (find / count /
    /// aggregate without `$out`/`$merge` / etc.) are accepted. Mongo
    /// has no per-session read-only mode, so this orchestrator-level
    /// check is the only guard.
    pub allow_write: bool,
}

impl MongoRequest {
    /// Build a typed MongoRequest by validating + draining a Config.
    ///
    /// `config.host` and `config.database` are required (returns
    /// `Error::Config` if missing). `config.port` defaults to 27017.
    /// Mongo auth is optional — neither `config.user` nor
    /// `config.password` is required at construction time (the server
    /// may reject unauthenticated commands at runtime).
    /// `allow_write` defaults to false; callers opt in explicitly.
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
    fn test_from_config_errors_on_missing_host() {
        let config = Config {
            database: Some("d".to_string()),
            ..Default::default()
        };
        let err = MongoRequest::from_config(config, "{}".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }

    #[test]
    fn test_from_config_errors_on_missing_database() {
        let config = Config {
            host: Some("h".to_string()),
            ..Default::default()
        };
        let err = MongoRequest::from_config(config, "{}".to_string()).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("database")));
    }

    #[test]
    fn test_from_config_succeeds_with_all_fields() {
        let config = Config {
            host: Some("h".to_string()),
            database: Some("d".to_string()),
            user: Some("u".to_string()),
            password: Some("p".to_string()),
            port: Some(27018),
            ..Default::default()
        };
        let req = MongoRequest::from_config(config, "{}".to_string()).unwrap();
        assert_eq!(req.host, "h");
        assert_eq!(req.port, 27018);
        assert_eq!(req.user.as_deref(), Some("u"));
        assert_eq!(req.password.as_deref(), Some("p"));
        assert_eq!(req.database, "d");
    }

    #[test]
    fn test_from_config_defaults_port_and_optional_auth() {
        // Mongo auth is optional — host + database alone is enough.
        let config = Config {
            host: Some("h".to_string()),
            database: Some("d".to_string()),
            ..Default::default()
        };
        let req = MongoRequest::from_config(config, "{}".to_string()).unwrap();
        assert_eq!(req.port, 27017);
        assert!(req.user.is_none());
        assert!(req.password.is_none());
        assert!(!req.allow_write);
    }

    #[tokio::test]
    async fn test_execute_rejects_write_without_allow_write() {
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
        assert!(
            matches!(err, Error::Service(ref msg) if msg.contains("--allow-write")),
            "expected --allow-write hint, got {err:?}"
        );
    }
}
