//! HTTP orchestrator: parse URL, build tunnel pointing at it, dispatch
//! into `tools4a_http::execute`. CLI handler and MCP `http_exec` tool
//! both delegate here.
//!
//! Unlike MySQL / Redis orchestrators, HTTP does NOT have a
//! `from_config` constructor — there's no Profile/YAML support yet
//! (Phase 6 deferred), so the bin builds `HttpRequestSpec` directly
//! from CLI flags / JSON params and calls
//! `HttpOrchestrator::execute(req, tunnel)`.

use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools4a_http::{HttpRequestSpec, execute as http_execute};

pub struct HttpOrchestrator;

#[async_trait]
impl Service for HttpOrchestrator {
    type Request = HttpRequestSpec;

    async fn execute(
        req: HttpRequestSpec,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        let parsed = reqwest::Url::parse(&req.url)
            .map_err(|e| Error::Config(format!("invalid URL '{}': {e}", req.url)))?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(Error::Config(format!(
                "URL '{}' uses an unsupported scheme '{scheme}' (need http/https)",
                req.url
            )));
        }
        let url_host = parsed
            .host_str()
            .ok_or_else(|| Error::Config(format!("URL '{}' has no host", req.url)))?
            .to_string();
        let url_port = parsed.port_or_known_default().ok_or_else(|| {
            Error::Config(format!(
                "URL '{}' has no port and the scheme provides no default",
                req.url
            ))
        })?;

        let tunnel: Box<dyn Tunnel> = match tunnel_config {
            None | Some(TunnelConfig::Direct) => {
                Box::new(DirectTunnel::new(url_host.clone(), url_port))
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
                    url_host.clone(),
                    url_port,
                )?)
            }
        };

        http_execute(tunnel, url_host, url_port, req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools4a_http::HttpAuth;

    fn empty_req(url: &str) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "GET".to_string(),
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
            auth: HttpAuth::None,
            insecure: false,
        }
    }

    #[tokio::test]
    async fn test_execute_errors_on_invalid_url() {
        let err = HttpOrchestrator::execute(empty_req("not a url"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("invalid URL")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_unsupported_scheme() {
        let err = HttpOrchestrator::execute(empty_req("ftp://example.com/x"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("unsupported scheme")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_file_scheme() {
        // file:// is parseable but is not http/https.
        let err = HttpOrchestrator::execute(empty_req("file:///tmp/x"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("unsupported scheme")));
    }
}
