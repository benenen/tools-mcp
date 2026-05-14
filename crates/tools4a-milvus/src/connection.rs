//! Build a connected `milvus::Client`. Auth via `username()/password()`
//! (Milvus 2.x basic auth). Timeout is per-call elsewhere; this is just
//! the RPC client construction.

use std::time::Duration;

use milvus::client::{Client, ClientBuilder};
use tools4a_core::{Error, Result};

/// Parameters for connecting to a Milvus daemon.
#[derive(Debug, Clone)]
pub struct ConnectParams {
    /// Final URI, e.g. `http://127.0.0.1:19530` or
    /// `https://milvus.example.com:19530`. The orchestrator builds this
    /// (substituting `127.0.0.1:<tunnel-port>` when tunneled).
    pub uri: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// gRPC connect timeout. Defaults to 10s if `None`.
    pub timeout: Option<Duration>,
}

pub async fn connect_milvus(p: &ConnectParams) -> Result<Client> {
    let mut builder = ClientBuilder::new(p.uri.clone());
    if let Some(u) = &p.username {
        builder = builder.username(u);
    }
    if let Some(pw) = &p.password {
        builder = builder.password(pw);
    }
    if let Some(t) = p.timeout {
        builder = builder.timeout(t);
    }
    builder
        .build()
        .await
        .map_err(|e| Error::Connection(format!("milvus connect to {}: {e}", p.uri)))
}
