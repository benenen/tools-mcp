//! Tiny RabbitMQ Management API client built on `reqwest`. Handles:
//! - base URL (scheme://host:port)
//! - basic-auth header (user/password)
//! - vhost URL encoding (`/` → `%2F`)
//! - optional `resolve()` DNS override for tunneled mode (mirrors the
//!   trick the `http` leaf uses to keep SNI/cert verify pointed at the
//!   original host while connecting to 127.0.0.1:<tunnel-port>)
//! - optional `danger_accept_invalid_certs` for `--insecure`

use std::net::SocketAddr;
use std::str::FromStr;

use reqwest::Client;
use serde_json::Value;
use tools4a_core::{Error, Result};

/// A configured client + base URL + auth pair, ready to make requests
/// against the RabbitMQ management API.
pub struct RabbitmqConnection {
    client: Client,
    /// `http(s)://host:port` — no trailing slash.
    base: String,
    user: String,
    password: String,
}

/// Connection parameters before we know what the tunnel resolved to.
#[derive(Debug, Clone)]
pub struct ConnectParams {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub insecure: bool,
    /// If `Some`, install a `resolve(host, addr)` override so reqwest
    /// connects to `addr` while keeping `host` for SNI / Host header /
    /// cert verification.
    pub resolve_override: Option<SocketAddr>,
}

impl RabbitmqConnection {
    pub fn build(p: &ConnectParams) -> Result<Self> {
        let mut builder = Client::builder()
            .danger_accept_invalid_certs(p.insecure)
            .user_agent(concat!("tools4a-rabbitmq/", env!("CARGO_PKG_VERSION")));
        if let Some(addr) = p.resolve_override {
            builder = builder.resolve(&p.host, addr);
        }
        let client = builder
            .build()
            .map_err(|e| Error::Service(format!("rabbitmq client init: {e}")))?;
        let base = format!("{}://{}:{}", p.scheme, p.host, p.port);
        Ok(Self {
            client,
            base,
            user: p.user.clone(),
            password: p.password.clone(),
        })
    }

    /// GET `path` (path must start with `/`). Returns parsed JSON.
    pub async fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
            .map_err(|e| Error::Service(format!("rabbitmq GET {path}: {e}")))?;
        Self::extract_json(resp, path).await
    }

    /// POST `path` with a JSON body. Returns parsed JSON.
    pub async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.user, Some(&self.password))
            .json(body)
            .send()
            .await
            .map_err(|e| Error::Service(format!("rabbitmq POST {path}: {e}")))?;
        Self::extract_json(resp, path).await
    }

    async fn extract_json(resp: reqwest::Response, path: &str) -> Result<Value> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Service(format!(
                "rabbitmq {path}: HTTP {} — {body}",
                status.as_u16()
            )));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| Error::Service(format!("rabbitmq {path}: parse JSON failed: {e}")))?;
        Ok(v)
    }
}

/// URL-encode a vhost name (default vhost `/` becomes `%2F`).
pub fn encode_vhost(vhost: &str) -> String {
    urlencoding::encode(vhost).into_owned()
}

/// Helper to turn `"host:port"` into a `SocketAddr` for the resolve()
/// override. Fails if the host isn't an IP literal — which is fine
/// because the tunnel always returns `127.0.0.1:<port>`.
pub fn parse_resolve_addr(host: &str, port: u16) -> Result<SocketAddr> {
    SocketAddr::from_str(&format!("{host}:{port}")).map_err(|e| {
        Error::Connection(format!(
            "tunnel endpoint {host}:{port} is not a SocketAddr (need IP:port): {e}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_default_vhost() {
        assert_eq!(encode_vhost("/"), "%2F");
    }

    #[test]
    fn encode_named_vhost() {
        assert_eq!(encode_vhost("myapp"), "myapp");
    }

    #[test]
    fn encode_vhost_with_slash_in_middle() {
        assert_eq!(encode_vhost("team/prod"), "team%2Fprod");
    }
}
