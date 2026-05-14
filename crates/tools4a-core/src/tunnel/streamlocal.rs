//! SSH→Unix-socket tunnel. Binds a localhost TCP listener and forwards
//! each accepted connection through an SSH chain via a fresh
//! `direct-streamlocal@openssh.com` channel to a remote Unix domain
//! socket path. Equivalent to `ssh -L 127.0.0.1:N:/var/run/foo.sock host`.
//!
//! Lifecycle mirrors `SshTunnel`:
//! - `new()` records config (no IO).
//! - `establish()` builds the session chain, binds `127.0.0.1:0`,
//!   spawns an accept loop, and returns a TCP `TunnelEndpoint`.
//! - `close()` signals shutdown and disconnects sessions (Arc drop).
//!
//! Differs from `SshTunnel` only in the channel call: `direct-streamlocal`
//! instead of `direct-tcpip`. Local side is still TCP so existing service
//! consumers (mysql/pgsql/http/...) work unchanged once they opt in.

use crate::session::{AcceptAnyHostKey, build_session_chain};
use crate::{Error, Result, Tunnel, TunnelEndpoint};
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct StreamLocalTunnel {
    ssh_jumps: Vec<String>,
    ssh_user: String,
    ssh_password: Option<String>,
    ssh_key_path: Option<std::path::PathBuf>,
    ssh_port: u16,
    remote_socket_path: String,
    /// Optional fixed listen address. `None` → 127.0.0.1:0 (random).
    /// `Some` set via `with_listen_addr` for Phase 16 `tunnel-serve` daemon.
    listen_addr: Option<SocketAddr>,
    /// Set after establish() succeeds.
    state: Option<StreamLocalTunnelState>,
}

struct StreamLocalTunnelState {
    /// Drop signals all background tasks (listener + each forwarder) to stop.
    shutdown: tokio::sync::watch::Sender<bool>,
    /// JoinHandle for the listener-accept loop. Awaited in close() for
    /// bounded teardown.
    listener_task: tokio::task::JoinHandle<()>,
    /// SSH session handles, ordered client-to-target. Kept alive so the
    /// channels in flight don't drop their parents.
    _sessions: Vec<Arc<Mutex<russh::client::Handle<AcceptAnyHostKey>>>>,
}

impl StreamLocalTunnel {
    pub fn new(
        ssh_jumps: Vec<String>,
        ssh_user: String,
        ssh_password: Option<String>,
        ssh_key_path: Option<std::path::PathBuf>,
        ssh_port: u16,
        remote_socket_path: String,
    ) -> Result<Self> {
        if ssh_jumps.is_empty() {
            return Err(Error::Config(
                "StreamLocalTunnel requires at least one jump host".to_string(),
            ));
        }
        if remote_socket_path.is_empty() {
            return Err(Error::Config(
                "StreamLocalTunnel requires a non-empty remote_socket_path".to_string(),
            ));
        }
        Ok(Self {
            ssh_jumps,
            ssh_user,
            ssh_password,
            ssh_key_path,
            ssh_port,
            remote_socket_path,
            listen_addr: None,
            state: None,
        })
    }

    /// Override the default `127.0.0.1:0` listen binding with a specific
    /// address. Used by the `tunnel-serve` CLI for daemon mode.
    pub fn with_listen_addr(mut self, addr: SocketAddr) -> Self {
        self.listen_addr = Some(addr);
        self
    }
}

#[async_trait]
impl Tunnel for StreamLocalTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        if self.state.is_some() {
            return Err(Error::Connection(
                "StreamLocalTunnel::establish called twice".to_string(),
            ));
        }

        let sessions = build_session_chain(
            &self.ssh_jumps,
            &self.ssh_user,
            self.ssh_password.as_deref(),
            self.ssh_key_path.as_deref(),
            self.ssh_port,
        )
        .await?;
        let final_handle = Arc::clone(sessions.last().expect("chain has at least one session"));

        let bind: SocketAddr = self
            .listen_addr
            .unwrap_or_else(|| "127.0.0.1:0".parse().expect("static addr"));
        let listener = tokio::net::TcpListener::bind(bind)
            .await
            .map_err(Error::Io)?;
        let local = listener.local_addr().map_err(Error::Io)?;
        let local_port = local.port();
        let local_host = local.ip().to_string();

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let socket_path = self.remote_socket_path.clone();

        let listener_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    accepted = listener.accept() => {
                        let (stream, _) = match accepted {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("streamlocal tunnel: listener accept failed: {e}");
                                break;
                            }
                        };
                        let path = socket_path.clone();
                        let session = Arc::clone(&final_handle);
                        tokio::spawn(async move {
                            if let Err(e) = forward_one(session, path, stream).await {
                                eprintln!("streamlocal tunnel: forward connection failed: {e}");
                            }
                        });
                    }
                }
            }
        });

        self.state = Some(StreamLocalTunnelState {
            shutdown: shutdown_tx,
            listener_task,
            _sessions: sessions,
        });

        Ok(TunnelEndpoint {
            host: local_host,
            port: local_port,
        })
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(state) = self.state.take() {
            let _ = state.shutdown.send(true);
            let _ = state.listener_task.await;
            // _sessions drop here, closing each SSH connection in reverse order.
        }
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.state.is_some()
    }
}

/// Bridge `local_stream` ⟷ direct-streamlocal channel from `session` to `socket_path`.
async fn forward_one(
    session: Arc<Mutex<russh::client::Handle<AcceptAnyHostKey>>>,
    socket_path: String,
    mut local_stream: tokio::net::TcpStream,
) -> Result<()> {
    let channel = session
        .lock()
        .await
        .channel_open_direct_streamlocal(socket_path.clone())
        .await
        .map_err(|e| {
            Error::Connection(format!(
                "open direct-streamlocal to {socket_path} failed: {e}"
            ))
        })?;
    // ChannelStream is not Unpin (ChannelTx contains a Pin<Box<dyn Future>>),
    // so we box-pin it before calling copy_bidirectional.
    let mut channel_stream = Box::pin(channel.into_stream());
    tokio::io::copy_bidirectional(&mut local_stream, &mut channel_stream)
        .await
        .map_err(Error::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rejects_empty_jumps() {
        assert!(matches!(
            StreamLocalTunnel::new(
                vec![],
                "u".into(),
                None,
                None,
                22,
                "/var/run/docker.sock".into()
            ),
            Err(Error::Config(_))
        ));
    }

    #[test]
    fn test_new_rejects_empty_socket_path() {
        assert!(matches!(
            StreamLocalTunnel::new(
                vec!["bastion.com".into()],
                "u".into(),
                None,
                None,
                22,
                String::new()
            ),
            Err(Error::Config(_))
        ));
    }

    #[test]
    fn test_new_accepts_valid_input() {
        let t = StreamLocalTunnel::new(
            vec!["bastion.com".into()],
            "u".into(),
            Some("p".into()),
            None,
            22,
            "/var/run/docker.sock".into(),
        )
        .unwrap();
        assert!(!t.is_active());
    }

    #[tokio::test]
    async fn test_state_starts_inactive() {
        let t = StreamLocalTunnel::new(
            vec!["bastion".into()],
            "u".into(),
            Some("p".into()),
            None,
            22,
            "/var/run/docker.sock".into(),
        )
        .unwrap();
        assert!(!t.is_active());
    }
}
