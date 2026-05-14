//! `MilvusOrchestrator` — Service impl. Builds an SshTunnel if requested,
//! points the gRPC URI at the tunnel local endpoint, gates write actions
//! behind `allow_write`, dispatches via `run::run`.

use std::time::Duration;

use crate::actions::MilvusAction;
use crate::connection::{ConnectParams, connect_milvus};
use crate::run::run as dispatch_action;
use async_trait::async_trait;
use tools4a_core::{
    Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig, apply_with_timeout,
    build_tunnel, resolve_effective_timeout,
};

pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_PORT: u16 = 19530;

#[derive(Debug, Clone)]
pub struct MilvusRequest {
    pub action: MilvusAction,
    /// `http` or `https`. Default `http`.
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Write actions require this to be `true`.
    pub allow_write: bool,
    pub timeout_secs: Option<u64>,
    pub max_timeout_secs: Option<u64>,
}

pub struct MilvusOrchestrator;

#[async_trait]
impl Service for MilvusOrchestrator {
    type Request = MilvusRequest;

    async fn execute(
        req: MilvusRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        if !req.action.is_readonly() && !req.allow_write {
            return Err(Error::Service(format!(
                "milvus {}: write action requires --allow-write (CLI) / allow_write=true (MCP)",
                req.action.name()
            )));
        }
        if req.scheme != "http" && req.scheme != "https" {
            return Err(Error::Config(format!(
                "milvus: scheme '{}' is not supported (need http or https)",
                req.scheme
            )));
        }

        // Build tunnel (if any). When a tunnel is in use, the URI points
        // at the tunnel's local TCP endpoint directly (lesson from Phase 17:
        // gRPC clients in this fork likely have the same IP-literal issue
        // reqwest's resolve() trick exhibits — keep things simple).
        let mut tunnel: Box<dyn Tunnel> = build_tunnel(req.host.clone(), req.port, tunnel_config)?;
        let endpoint = tunnel.establish().await?;
        let uri = format!("{}://{}:{}", req.scheme, endpoint.host, endpoint.port);

        let deadline =
            resolve_effective_timeout(req.timeout_secs, DEFAULT_TIMEOUT_SECS, req.max_timeout_secs);

        let params = ConnectParams {
            uri,
            username: req.username.clone(),
            password: req.password.clone(),
            timeout: Some(Duration::from_secs(deadline.effective_secs)),
        };
        let client = connect_milvus(&params).await?;

        let mut result = apply_with_timeout(deadline, dispatch_action(&client, req.action)).await;
        let _ = tunnel.close().await;
        if let Ok(ref mut r) = result
            && let Some(w) = deadline.clamp_warning()
        {
            r.push_warning(w);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_action_rejected_without_allow_write() {
        let req = MilvusRequest {
            action: MilvusAction::DropCollection {
                name: "x".to_string(),
            },
            scheme: "http".to_string(),
            host: "127.0.0.1".to_string(),
            port: DEFAULT_PORT,
            username: None,
            password: None,
            allow_write: false,
            timeout_secs: None,
            max_timeout_secs: None,
        };
        let err = MilvusOrchestrator::execute(req, None).await.unwrap_err();
        assert!(matches!(err, Error::Service(ref m) if m.contains("--allow-write")));
    }

    #[tokio::test]
    async fn unsupported_scheme_rejected() {
        let req = MilvusRequest {
            action: MilvusAction::ListCollections,
            scheme: "ftp".to_string(),
            host: "h".to_string(),
            port: 1,
            username: None,
            password: None,
            allow_write: false,
            timeout_secs: None,
            max_timeout_secs: None,
        };
        let err = MilvusOrchestrator::execute(req, None).await.unwrap_err();
        assert!(matches!(err, Error::Config(ref m) if m.contains("scheme")));
    }
}
