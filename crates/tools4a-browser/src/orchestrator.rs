//! BrowserOrchestrator — `Service` impl for the browser tool.
//!
//! Validates the tunnel kind (Phase 1 only allows None / Direct;
//! TunnelConfig::Ssh is rejected with an explicit Phase 2 deferral
//! message), then dispatches into `execute`.

use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig};

use crate::execute::execute;
use crate::request::BrowserRequest;

pub struct BrowserOrchestrator;

#[async_trait]
impl Service for BrowserOrchestrator {
    type Request = BrowserRequest;

    async fn execute(req: Self::Request, tunnel: Option<TunnelConfig>) -> Result<ExecutionResult> {
        match tunnel {
            None | Some(TunnelConfig::Direct) => execute(req).await,
            Some(TunnelConfig::Ssh { .. }) => Err(Error::Config(
                "tunnel=ssh is not supported for the browser tool in Phase 1. \
                 The current SSH tunnel forwards a single TCP port (direct-tcpip), \
                 which doesn't fit a full browser's network stack (cookies / SNI / \
                 Host header / sub-resources). Phase 2 will add SOCKS5 routing \
                 through SSH. As a workaround, run `ssh -D 1080 <bastion>` yourself \
                 and pass `--proxy socks5://127.0.0.1:1080` (CLI) or `\"proxy\": \
                 \"socks5://127.0.0.1:1080\"` (MCP)."
                    .to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> BrowserRequest {
        BrowserRequest {
            subcommand: "snapshot".into(),
            args: Vec::new(),
            session: None,
            proxy: None,
            proxy_bypass: None,
            browser_args: None,
            bin: Some(std::path::PathBuf::from("/nonexistent/ab")),
        }
    }

    #[tokio::test]
    async fn rejects_ssh_tunnel_with_phase2_message() {
        let err = BrowserOrchestrator::execute(
            req(),
            Some(TunnelConfig::Ssh {
                ssh_jumps: vec!["bastion.example.com".to_string()],
                ssh_user: "admin".to_string(),
                ssh_password: None,
                ssh_key_path: None,
                ssh_port: 22,
            }),
        )
        .await
        .unwrap_err();
        match err {
            Error::Config(m) => {
                assert!(m.contains("Phase 2"), "got: {m}");
                assert!(m.contains("socks5://"), "got: {m}");
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn accepts_none_tunnel() {
        // Delegates to execute, which delegates to BrowserExec; the
        // missing binary surfaces as Error::Config "not found",
        // confirming the guard didn't short-circuit.
        let err = BrowserOrchestrator::execute(req(), None).await.unwrap_err();
        match err {
            Error::Config(m) => assert!(m.contains("not found"), "got: {m}"),
            other => panic!("expected Config, got {other:?}"),
        }
    }
}
