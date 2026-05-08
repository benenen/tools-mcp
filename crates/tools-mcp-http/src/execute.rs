//! Top-level entry: build a reqwest client (with tunnel resolve override
//! when needed), send a single HTTP request, return the structured result.

use std::net::SocketAddr;
use std::str::FromStr;
use tools_mcp_core::{Error, ExecutionResult, Result, Tunnel};

use crate::executor::HttpExecutor;
use crate::request::HttpRequestSpec;

/// Send a single HTTP request through `tunnel`. The tunnel's endpoint is
/// only consulted to override DNS for the URL's host — when the endpoint
/// matches the URL host:port (i.e. DirectTunnel returning the same address),
/// reqwest performs a normal DNS lookup. When the tunnel rewrote the address
/// (i.e. SshTunnel returning 127.0.0.1:<local-port>), `resolve()` points the
/// URL host at the tunnel's local listener while preserving Host header,
/// TLS SNI, and cert verification against the original hostname.
pub async fn execute(
    mut tunnel: Box<dyn Tunnel>,
    url_host: String,
    url_port: u16,
    req: HttpRequestSpec,
) -> Result<ExecutionResult> {
    let endpoint = tunnel.establish().await?;

    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(req.insecure)
        .user_agent(concat!("tools-mcp/", env!("CARGO_PKG_VERSION")));

    let needs_resolve = endpoint.host != url_host || endpoint.port != url_port;
    if needs_resolve {
        let addr_str = format!("{}:{}", endpoint.host, endpoint.port);
        let addr = SocketAddr::from_str(&addr_str).map_err(|e| {
            Error::Connection(format!(
                "tunnel endpoint {addr_str} is not a SocketAddr (need IP:port for DNS override): {e}"
            ))
        })?;
        builder = builder.resolve(&url_host, addr);
    }

    let client = builder
        .build()
        .map_err(|e: reqwest::Error| Error::Service(format!("HTTP client init: {e}")))?;

    let result = HttpExecutor::run(&client, req).await;

    let _ = tunnel.close().await;
    result
}
