//! Orchestrator: take an HttpRequestSpec + an optional TunnelConfig,
//! parse the URL, build the right tunnel, dispatch into tools_mcp_http.
//! CLI handler and MCP `http_exec` tool both delegate here so both
//! presentation layers go through identical request + tunnel teardown.

use crate::config::TunnelConfig;
use crate::tunnel::{DirectTunnel, SshTunnel};
use tools_mcp_core::{Error, ExecutionResult, Result, Tunnel};
use tools_mcp_http::{HttpRequestSpec, execute as http_execute};

pub async fn execute(
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

#[cfg(test)]
mod tests {
    use super::*;
    use tools_mcp_http::HttpAuth;

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
        let err = execute(empty_req("not a url"), None).await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("invalid URL")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_unsupported_scheme() {
        let err = execute(empty_req("ftp://example.com/x"), None).await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("unsupported scheme")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_file_scheme() {
        // file:// is parseable but is not http/https.
        let err = execute(empty_req("file:///tmp/x"), None).await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("unsupported scheme")));
    }
}
