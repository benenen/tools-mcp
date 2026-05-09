//! HTTP orchestrator: parse URL, build tunnel pointing at it, dispatch
//! into `tools4a_http::execute`.
//!
//! Unlike MySQL / Pgsql / Mongo orchestrators, HTTP does NOT have a
//! `from_config` constructor — there's no Profile/YAML support
//! (deliberately deferred), so the bin builds `HttpRequestSpec` directly
//! from CLI flags / JSON params.

use crate::execute as http_execute;
use crate::request::HttpRequestSpec;
use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig, build_tunnel};

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

        let tunnel = build_tunnel(url_host.clone(), url_port, tunnel_config)?;

        http_execute(tunnel, url_host, url_port, req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::HttpAuth;

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
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("invalid URL")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_unsupported_scheme() {
        let err = HttpOrchestrator::execute(empty_req("ftp://example.com/x"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("unsupported scheme")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_file_scheme() {
        let err = HttpOrchestrator::execute(empty_req("file:///tmp/x"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Config(ref msg) if msg.contains("unsupported scheme")));
    }
}
