//! Connector trait — abstracts "open a byte-stream to (host, port)"
//! so the SOCKS5 server can be tested without russh.
//!
//! `SshConnector` is the production impl: each `connect` call opens
//! a new `direct-tcpip` channel on the shared SSH session.

use async_trait::async_trait;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;

use crate::session::AcceptAnyHostKey;
use crate::{Error, Result};

/// Async stream the SOCKS server bidirectionally copies bytes
/// between (inbound TCP socket <-> outbound `Stream`).
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

#[async_trait]
pub trait Connector: Send + Sync {
    /// Open a stream to `host:port`. The implementor decides how to
    /// resolve, if at all — SshConnector forwards the literal name
    /// through SSH `direct-tcpip` so the bastion does the resolution.
    async fn connect(&self, host: &str, port: u16) -> Result<Pin<Box<dyn Stream>>>;
}

/// Production impl: open a russh `direct-tcpip` channel and wrap
/// it as an AsyncRead + AsyncWrite stream.
///
/// `session` is the last session in the SSH chain (the one whose
/// transport the channel rides on). It's wrapped in `Arc<Mutex>` so
/// concurrent SOCKS connections serialize on the brief channel-open
/// call; the resulting channels are independent and copy in parallel.
pub struct SshConnector {
    pub session: Arc<Mutex<russh::client::Handle<AcceptAnyHostKey>>>,
}

#[async_trait]
impl Connector for SshConnector {
    async fn connect(&self, host: &str, port: u16) -> Result<Pin<Box<dyn Stream>>> {
        let channel = self
            .session
            .lock()
            .await
            .channel_open_direct_tcpip(host.to_string(), port as u32, "127.0.0.1", 0u32)
            .await
            .map_err(|e| Error::Connection(format!("direct-tcpip open to {host}:{port}: {e}")))?;
        // ChannelStream is not Unpin (its writer holds Pin<Box<dyn Future>>),
        // so we box-pin it before handing it back as `dyn Stream`. This
        // mirrors how tunnel/ssh.rs wraps the stream before
        // `tokio::io::copy_bidirectional`.
        Ok(Box::pin(channel.into_stream()))
    }
}
