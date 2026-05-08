//! SSH session-chain primitives. Used by both `SshTunnel` (in the bin) and
//! `SshExec` (in this crate) to walk a chain of SSH jump hosts and end up
//! with one or more authenticated SSH sessions.

use async_trait::async_trait;
use russh::client;
use russh::keys::key::PublicKey;
use std::sync::Arc;
use tokio::sync::Mutex;
use tools_mcp_core::{Error, Result};

/// russh client handler that accepts any server host key but logs a
/// fingerprint warning to stderr (matching openssh's
/// StrictHostKeyChecking=accept-new ergonomics). A future phase can add a
/// strict-checking variant backed by ~/.ssh/known_hosts.
#[allow(dead_code)]
pub struct AcceptAnyHostKey {
    pub label: String,
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
/// Note: passphrase-protected keys are not supported yet. The `None`
/// passphrase means unencrypted keys only.
pub async fn authenticate(
    handle: &mut client::Handle<AcceptAnyHostKey>,
    user: &str,
    password: Option<&str>,
    key_path: Option<&std::path::Path>,
) -> Result<()> {
    if let Some(path) = key_path {
        // load_secret_key returns russh::keys::key::KeyPair in russh-keys 0.46.
        // None = no passphrase (passphrase-protected keys deferred).
        let key = russh::keys::load_secret_key(path, None).map_err(|e| {
            Error::Connection(format!(
                "failed to load SSH key from '{}': {}",
                path.display(),
                e
            ))
        })?;
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

/// Open SSH session(s), one per jump host, chained via direct-tcpip.
/// Returns the chain in client→last-jump order; the last entry is the
/// session whose direct-tcpip channel can be used to reach the next hop
/// (or the final TCP/SSH target).
///
/// All hops share `user`/`password`/`key_path`/`port`. (Per-hop overrides
/// are deferred to a future phase.)
///
/// `jumps` must not be empty — caller validates.
pub async fn build_session_chain(
    jumps: &[String],
    user: &str,
    password: Option<&str>,
    key_path: Option<&std::path::Path>,
    port: u16,
) -> Result<Vec<Arc<Mutex<client::Handle<AcceptAnyHostKey>>>>> {
    let cfg = std::sync::Arc::new(client::Config::default());
    let mut sessions: Vec<Arc<Mutex<client::Handle<AcceptAnyHostKey>>>> =
        Vec::with_capacity(jumps.len());

    // Hop 0: TCP-connect directly.
    let first_jump = &jumps[0];
    let handler = AcceptAnyHostKey {
        label: first_jump.clone(),
    };
    let mut session = client::connect(cfg.clone(), (first_jump.as_str(), port), handler)
        .await
        .map_err(|e| Error::Connection(format!("SSH connect to {first_jump} failed: {e}")))?;
    authenticate(&mut session, user, password, key_path).await?;
    sessions.push(Arc::new(Mutex::new(session)));

    // Hop 1..N: each over a direct-tcpip channel of the prior session.
    for next_jump in jumps.iter().skip(1) {
        let prev = sessions.last().expect("at least one session");
        let channel = prev
            .lock()
            .await
            .channel_open_direct_tcpip(next_jump.clone(), port as u32, "127.0.0.1", 0u32)
            .await
            .map_err(|e| {
                Error::Connection(format!(
                    "open direct-tcpip to {next_jump}:{port} via prior hop failed: {e}"
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
        authenticate(&mut session, user, password, key_path).await?;
        sessions.push(Arc::new(Mutex::new(session)));
    }

    Ok(sessions)
}
