//! SOCKS5 server over an SSH session chain. Each accepted SOCKS
//! connection opens a fresh russh `direct-tcpip` channel to the
//! requested (host, port); the bastion does the actual TCP connect
//! and DNS resolution.
//!
//! Lifecycle:
//! - `new()` records the SSH chain config (no IO yet).
//! - `establish()` builds the session chain, binds a localhost TCP
//!   listener on a random port, spawns the accept loop, and returns
//!   `127.0.0.1:<port>` as the TunnelEndpoint.
//! - `close()` signals the accept loop and disconnects the session
//!   chain (via Arc drop).

use async_trait::async_trait;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::session::{AcceptAnyHostKey, build_session_chain};
use crate::tunnel::socks::connector::{Connector, SshConnector};
use crate::tunnel::socks::server::Socks5Server;
use crate::{Error, Result, Tunnel, TunnelEndpoint};

/// SOCKS5 tunnel: binds a localhost SOCKS5 listener whose CONNECT
/// requests open fresh `direct-tcpip` channels through an SSH chain.
/// Used by `BrowserOrchestrator` to give the browser tool the
/// shape-of-routing it needs (many on-demand target hosts, not one
/// fixed target like SshTunnel).
pub struct SocksTunnel {
    ssh_jumps: Vec<String>,
    ssh_user: String,
    ssh_password: Option<String>,
    ssh_key_path: Option<std::path::PathBuf>,
    ssh_port: u16,

    /// Set on `establish()`, cleared on `close()`.
    state: Option<SocksTunnelState>,
}

struct SocksTunnelState {
    /// Signals the accept loop to stop on the next iteration.
    shutdown: tokio::sync::watch::Sender<bool>,
    /// JoinHandle for the accept loop. Awaited in close() for bounded
    /// teardown.
    accept_task: JoinHandle<()>,
    /// SSH session chain — kept alive so the per-conn channels in
    /// flight don't lose their parents while the accept loop drains.
    _sessions: Vec<Arc<Mutex<russh::client::Handle<AcceptAnyHostKey>>>>,
}

impl SocksTunnel {
    pub fn new(
        ssh_jumps: Vec<String>,
        ssh_user: String,
        ssh_password: Option<String>,
        ssh_key_path: Option<std::path::PathBuf>,
        ssh_port: u16,
    ) -> Result<Self> {
        if ssh_jumps.is_empty() {
            return Err(Error::Config(
                "SocksTunnel requires at least one jump host".to_string(),
            ));
        }
        Ok(Self {
            ssh_jumps,
            ssh_user,
            ssh_password,
            ssh_key_path,
            ssh_port,
            state: None,
        })
    }
}

#[async_trait]
impl Tunnel for SocksTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        if self.state.is_some() {
            return Err(Error::Connection(
                "SocksTunnel::establish called twice".to_string(),
            ));
        }

        // 1. Build the SSH chain. Same helper SshTunnel uses.
        let sessions = build_session_chain(
            &self.ssh_jumps,
            &self.ssh_user,
            self.ssh_password.as_deref(),
            self.ssh_key_path.as_deref(),
            self.ssh_port,
        )
        .await?;
        let final_handle = Arc::clone(sessions.last().expect("chain has at least one session"));

        // 2. Bind localhost listener on a random port; capture it
        //    BEFORE spawning so we can return the endpoint synchronously.
        let listener = TcpListener::bind("127.0.0.1:0").await.map_err(Error::Io)?;
        let local_port = listener.local_addr().map_err(Error::Io)?.port();

        // 3. Spawn accept loop: one SshConnector shared across all
        //    inbound SOCKS conns; each accepted conn gets a
        //    Socks5Server::serve_one task that opens its own channel.
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let accept_task = tokio::spawn(async move {
            let connector: Arc<dyn Connector> = Arc::new(SshConnector {
                session: final_handle,
            });
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _peer)) => {
                                let c = Arc::clone(&connector);
                                tokio::spawn(async move {
                                    if let Err(e) = Socks5Server::serve_one(stream, c).await {
                                        eprintln!("socks tunnel: serve_one: {e}");
                                    }
                                });
                            }
                            Err(e) => {
                                eprintln!("socks tunnel: accept failed: {e}");
                                // Back off briefly so EMFILE etc don't spin.
                                tokio::time::sleep(
                                    std::time::Duration::from_millis(50),
                                ).await;
                            }
                        }
                    }
                }
            }
        });

        self.state = Some(SocksTunnelState {
            shutdown: shutdown_tx,
            accept_task,
            _sessions: sessions,
        });

        Ok(TunnelEndpoint {
            host: "127.0.0.1".to_string(),
            port: local_port,
        })
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(state) = self.state.take() {
            let _ = state.shutdown.send(true);
            // Best-effort wait for accept loop; per-conn copy tasks may
            // outlive this and finish when their channels EOF naturally
            // as the SSH session disconnects (via _sessions drop).
            let _ = state.accept_task.await;
        }
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.state.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_jumps() {
        assert!(matches!(
            SocksTunnel::new(vec![], "u".into(), None, None, 22),
            Err(Error::Config(_))
        ));
    }

    #[test]
    fn new_accepts_single_jump_inactive() {
        let t = SocksTunnel::new(
            vec!["bastion.com".into()],
            "u".into(),
            Some("p".into()),
            None,
            22,
        )
        .unwrap();
        assert!(!t.is_active());
    }
}
