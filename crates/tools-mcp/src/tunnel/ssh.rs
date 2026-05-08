use async_trait::async_trait;
use tools_mcp_core::{Error, Result, Tunnel, TunnelEndpoint};
use russh::client;
use russh::keys::key::PublicKey;
use std::sync::Arc;
use tokio::sync::Mutex;

/// SSH-jump tunnel. Establishes a chain of SSH sessions through
/// `ssh_jumps` (in client→target order) and exposes a local TCP
/// endpoint on 127.0.0.1 that forwards to `(target_host, target_port)`.
pub struct SshTunnel {
    ssh_jumps: Vec<String>,
    ssh_user: String,
    ssh_password: Option<String>,
    ssh_key_path: Option<std::path::PathBuf>,
    ssh_port: u16,
    target_host: String,
    target_port: u16,
    /// Set to Some after establish() succeeds.
    state: Option<SshTunnelState>,
}

struct SshTunnelState {
    /// Drop signals all background tasks (listener + each forwarder) to stop.
    shutdown: tokio::sync::watch::Sender<bool>,
    /// JoinHandle for the listener-accept loop. Dropped in close() after
    /// signaling shutdown to make close() bounded-time.
    listener_task: tokio::task::JoinHandle<()>,
    /// SSH session handles, ordered client-to-target. Kept alive so the
    /// channels in the listener task don't drop their parents.
    _sessions: Vec<Arc<Mutex<client::Handle<AcceptAnyHostKey>>>>,
}

/// russh client handler that accepts any server host key but logs a
/// fingerprint warning to stderr. Phase 2 simplification — Phase 3 will
/// add a strict-checking variant backed by ~/.ssh/known_hosts.
#[allow(dead_code)]
struct AcceptAnyHostKey {
    label: String,
}

#[async_trait]
impl client::Handler for AcceptAnyHostKey {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint();
        eprintln!(
            "warning: accepting unverified host key for {}: {}",
            self.label, fingerprint
        );
        Ok(true)
    }
}

/// Authenticate `handle` using key path first (if provided), then
/// password. Returns Err if neither succeeds or neither is supplied.
///
/// Note: passphrase-protected keys are not supported yet (Phase 3 can
/// add `--ssh-key-passphrase`). The `None` passphrase means unencrypted
/// keys only.
#[allow(dead_code)]
async fn authenticate(
    handle: &mut client::Handle<AcceptAnyHostKey>,
    user: &str,
    password: Option<&str>,
    key_path: Option<&std::path::Path>,
) -> Result<()> {
    if let Some(path) = key_path {
        // load_secret_key returns russh::keys::key::KeyPair in russh-keys 0.46.
        // None = no passphrase (passphrase-protected keys deferred to Phase 3).
        let key = russh::keys::load_secret_key(path, None).map_err(|e| {
            Error::Connection(format!(
                "failed to load SSH key from '{}': {}",
                path.display(),
                e
            ))
        })?;
        // authenticate_publickey takes Arc<key::KeyPair> and returns Result<bool>.
        let success = handle
            .authenticate_publickey(user, std::sync::Arc::new(key))
            .await
            .map_err(|e| Error::Connection(format!("SSH publickey auth failed: {e}")))?;
        if success {
            return Ok(());
        }
        // fall through to password if provided
    }

    if let Some(pw) = password {
        // authenticate_password returns Result<bool> in russh 0.46.
        let success = handle
            .authenticate_password(user, pw)
            .await
            .map_err(|e| Error::Connection(format!("SSH password auth failed: {e}")))?;
        if success {
            return Ok(());
        }
        return Err(Error::Connection(
            "SSH password authentication rejected".to_string(),
        ));
    }

    Err(Error::Connection(
        "SSH authentication failed: no usable credentials (provide --ssh-key-path or --ssh-password)".to_string(),
    ))
}

impl SshTunnel {
    pub fn new(
        ssh_jumps: Vec<String>,
        ssh_user: String,
        ssh_password: Option<String>,
        ssh_key_path: Option<std::path::PathBuf>,
        ssh_port: u16,
        target_host: String,
        target_port: u16,
    ) -> Result<Self> {
        if ssh_jumps.is_empty() {
            return Err(Error::Config(
                "SshTunnel requires at least one jump host".to_string(),
            ));
        }
        Ok(Self {
            ssh_jumps,
            ssh_user,
            ssh_password,
            ssh_key_path,
            ssh_port,
            target_host,
            target_port,
            state: None,
        })
    }
}

#[async_trait]
impl Tunnel for SshTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        if self.state.is_some() {
            return Err(Error::Connection(
                "SshTunnel::establish called twice".to_string(),
            ));
        }

        // Build SSH session chain: open the first session via TCP, then
        // chain subsequent sessions over direct-tcpip channels.
        let sessions = self.build_session_chain().await?;
        let final_handle = Arc::clone(sessions.last().expect("chain has at least one session"));

        // Bind local listener and capture port BEFORE spawning so we can
        // return the endpoint synchronously.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(Error::Io)?;
        let local_port = listener.local_addr().map_err(Error::Io)?.port();

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let target_host = self.target_host.clone();
        let target_port = self.target_port;

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
                                eprintln!("ssh tunnel: listener accept failed: {e}");
                                break;
                            }
                        };
                        let host = target_host.clone();
                        let session = Arc::clone(&final_handle);
                        tokio::spawn(async move {
                            if let Err(e) = forward_one(session, host, target_port, stream).await {
                                eprintln!("ssh tunnel: forward connection failed: {e}");
                            }
                        });
                    }
                }
            }
        });

        self.state = Some(SshTunnelState {
            shutdown: shutdown_tx,
            listener_task,
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
            // Best-effort wait for listener task; ignore JoinError.
            let _ = state.listener_task.await;
            // _sessions drop here, closing each SSH connection in reverse order.
        }
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.state.is_some()
    }
}

impl SshTunnel {
    /// Open SSH session(s), one per jump host, chained via direct-tcpip.
    /// Returns the chain in client→target order; the last entry is the
    /// session that should open the final direct-tcpip channel to target.
    async fn build_session_chain(
        &self,
    ) -> Result<Vec<Arc<Mutex<client::Handle<AcceptAnyHostKey>>>>> {
        let cfg = std::sync::Arc::new(client::Config::default());
        let mut sessions: Vec<Arc<Mutex<client::Handle<AcceptAnyHostKey>>>> =
            Vec::with_capacity(self.ssh_jumps.len());

        // Hop 0: TCP-connect directly.
        let first_jump = &self.ssh_jumps[0];
        let handler = AcceptAnyHostKey {
            label: first_jump.clone(),
        };
        let mut session =
            client::connect(cfg.clone(), (first_jump.as_str(), self.ssh_port), handler)
                .await
                .map_err(|e| {
                    Error::Connection(format!("SSH connect to {first_jump} failed: {e}"))
                })?;
        authenticate(
            &mut session,
            &self.ssh_user,
            self.ssh_password.as_deref(),
            self.ssh_key_path.as_deref(),
        )
        .await?;
        sessions.push(Arc::new(Mutex::new(session)));

        // Hop 1..N: each over a direct-tcpip channel of the prior session.
        for next_jump in self.ssh_jumps.iter().skip(1) {
            let prev = sessions.last().expect("at least one session");
            let channel = prev
                .lock()
                .await
                .channel_open_direct_tcpip(
                    next_jump.clone(),
                    self.ssh_port as u32,
                    "127.0.0.1",
                    0u32,
                )
                .await
                .map_err(|e| {
                    Error::Connection(format!(
                        "open direct-tcpip to {next_jump}:{} via prior hop failed: {e}",
                        self.ssh_port
                    ))
                })?;
            // ChannelStream is not Unpin; box-pin so connect_stream's bound holds.
            let stream = Box::pin(channel.into_stream());

            let handler = AcceptAnyHostKey {
                label: next_jump.clone(),
            };
            let mut session = client::connect_stream(cfg.clone(), stream, handler)
                .await
                .map_err(|e| {
                    Error::Connection(format!("SSH connect to {next_jump} (chained) failed: {e}"))
                })?;
            authenticate(
                &mut session,
                &self.ssh_user,
                self.ssh_password.as_deref(),
                self.ssh_key_path.as_deref(),
            )
            .await?;
            sessions.push(Arc::new(Mutex::new(session)));
        }

        Ok(sessions)
    }
}

/// Bridge `local_stream` ⟷ direct-tcpip channel from `session` to `target_host:target_port`.
async fn forward_one(
    session: Arc<Mutex<client::Handle<AcceptAnyHostKey>>>,
    target_host: String,
    target_port: u16,
    mut local_stream: tokio::net::TcpStream,
) -> Result<()> {
    let channel = session
        .lock()
        .await
        .channel_open_direct_tcpip(target_host.clone(), target_port as u32, "127.0.0.1", 0u32)
        .await
        .map_err(|e| {
            Error::Connection(format!(
                "open direct-tcpip to {target_host}:{target_port} failed: {e}"
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
            SshTunnel::new(vec![], "u".into(), None, None, 22, "t".into(), 3306),
            Err(Error::Config(_))
        ));
    }

    #[test]
    fn test_new_accepts_single_jump() {
        let t = SshTunnel::new(
            vec!["bastion.com".into()],
            "u".into(),
            Some("p".into()),
            None,
            22,
            "mysql.internal".into(),
            3306,
        )
        .unwrap();
        assert!(!t.is_active());
    }

    #[tokio::test]
    async fn test_ssh_tunnel_state_starts_inactive() {
        let t = SshTunnel::new(
            vec!["bastion".into()],
            "u".into(),
            Some("p".into()),
            None,
            22,
            "target".into(),
            3306,
        )
        .unwrap();
        assert!(!t.is_active());
    }
}
