//! MySQL orchestrator: typed request → `tools_mcp_mysql::execute` with
//! the right tunnel built from the request's tunnel config.

use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools_mcp_core::{ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools_mcp_mysql::{MysqlParams, execute as mysql_execute};

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
