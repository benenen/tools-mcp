//! `RabbitmqOrchestrator` — builds a tunnel if requested, sets up the
//! reqwest client with the right resolve override, and dispatches the
//! typed action through `run::run`.

use crate::actions::RabbitmqAction;
use crate::connection::{ConnectParams, RabbitmqConnection, parse_resolve_addr};
use crate::run::run as dispatch_action;
use async_trait::async_trait;
use tools4a_core::{
    Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig, apply_with_timeout,
    build_tunnel, resolve_effective_timeout,
};

pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_HTTP_PORT: u16 = 15672;
pub const DEFAULT_HTTPS_PORT: u16 = 15671;

#[derive(Debug, Clone)]
pub struct RabbitmqRequest {
    pub action: RabbitmqAction,
    /// "http" or "https". Default `http`.
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// Skip TLS cert verification. Useful for HTTPS + self-signed certs
    /// or HTTPS through a tunnel where the cert doesn't cover 127.0.0.1.
    pub insecure: bool,
    pub timeout_secs: Option<u64>,
    pub max_timeout_secs: Option<u64>,
}

pub struct RabbitmqOrchestrator;

#[async_trait]
impl Service for RabbitmqOrchestrator {
    type Request = RabbitmqRequest;

    async fn execute(
        req: RabbitmqRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        if req.scheme != "http" && req.scheme != "https" {
            return Err(Error::Config(format!(
                "rabbitmq: scheme '{}' is not supported (need http or https)",
                req.scheme
            )));
        }

        // Build tunnel (if any) and figure out the resolve override.
        let mut tunnel: Box<dyn Tunnel> = build_tunnel(req.host.clone(), req.port, tunnel_config)?;
        let endpoint = tunnel.establish().await?;

        let resolve_override = if endpoint.host != req.host || endpoint.port != req.port {
            Some(parse_resolve_addr(&endpoint.host, endpoint.port)?)
        } else {
            None
        };

        let params = ConnectParams {
            scheme: req.scheme.clone(),
            host: req.host.clone(),
            port: req.port,
            user: req.user.clone(),
            password: req.password.clone(),
            insecure: req.insecure,
            resolve_override,
        };
        let conn = RabbitmqConnection::build(&params)?;

        let deadline =
            resolve_effective_timeout(req.timeout_secs, DEFAULT_TIMEOUT_SECS, req.max_timeout_secs);

        let mut result = apply_with_timeout(deadline, dispatch_action(&conn, req.action)).await;
        // Tear the tunnel down before returning either branch.
        let _ = tunnel.close().await;
        if let Ok(ref mut r) = result
            && let Some(w) = deadline.clamp_warning()
        {
            r.push_warning(w);
        }
        result
    }
}

/// Pick a default port based on scheme when the caller didn't set one.
pub fn default_port_for(scheme: &str) -> u16 {
    if scheme == "https" {
        DEFAULT_HTTPS_PORT
    } else {
        DEFAULT_HTTP_PORT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn errors_on_unsupported_scheme() {
        let req = RabbitmqRequest {
            action: RabbitmqAction::Overview,
            scheme: "ftp".to_string(),
            host: "h".into(),
            port: 1,
            user: "u".into(),
            password: "p".into(),
            insecure: false,
            timeout_secs: None,
            max_timeout_secs: None,
        };
        let err = RabbitmqOrchestrator::execute(req, None).await.unwrap_err();
        assert!(matches!(err, Error::Config(ref m) if m.contains("scheme")));
    }

    #[test]
    fn default_port_resolves() {
        assert_eq!(default_port_for("http"), 15672);
        assert_eq!(default_port_for("https"), 15671);
    }
}
