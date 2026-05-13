//! BrowserOrchestrator — `Service` impl for the browser tool.
//!
//! Tunnel behavior:
//!   - None / Direct: spawn agent-browser as-is.
//!   - Ssh: build a SocksTunnel, inject `--proxy socks5://<endpoint>`
//!     into the request, then run; tear the tunnel down on exit.

use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, SocksTunnel, Tunnel, TunnelConfig};

use crate::execute::execute;
use crate::request::BrowserRequest;

pub struct BrowserOrchestrator;

#[async_trait]
impl Service for BrowserOrchestrator {
    type Request = BrowserRequest;

    async fn execute(
        mut req: Self::Request,
        tunnel: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        match tunnel {
            None | Some(TunnelConfig::Direct) => execute(req).await,
            Some(TunnelConfig::Ssh {
                ssh_jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
            }) => {
                if req.proxy.is_some() {
                    return Err(Error::Config(
                        "tunnel=ssh and an explicit `proxy` field conflict: \
                         tools4a injects `--proxy socks5://...` when ssh is set. \
                         Pick one — drop `proxy` and let tools4a do it, or use \
                         tunnel=direct + your own proxy."
                            .into(),
                    ));
                }

                let mut t = SocksTunnel::new(
                    ssh_jumps,
                    ssh_user,
                    ssh_password,
                    ssh_key_path.map(std::path::PathBuf::from),
                    ssh_port,
                )?;
                let endpoint = t.establish().await?;
                req.proxy = Some(format!("socks5://{}:{}", endpoint.host, endpoint.port));

                let result = execute(req).await;

                // Tear down regardless of outcome. Errors here don't
                // override the execute() result; the call already
                // happened, so the user-facing outcome is whatever
                // execute returned.
                if let Err(e) = t.close().await {
                    eprintln!("BrowserOrchestrator: SocksTunnel close: {e}");
                }
                result
            }
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
    async fn errors_when_user_proxy_conflicts_with_ssh_tunnel() {
        let mut r = req();
        r.proxy = Some("socks5://example.com:1080".into());
        let err = BrowserOrchestrator::execute(
            r,
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
                assert!(m.contains("conflict"), "got: {m}");
                assert!(m.contains("socks5"), "got: {m}");
            }
            other => panic!("got {other:?}"),
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
