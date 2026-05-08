use crate::error::{Error, Result};
use crate::tunnel::traits::{Tunnel, TunnelEndpoint};
use async_trait::async_trait;
use russh::client;
use russh::keys::key::PublicKey;

/// SSH-jump tunnel. Establishes a chain of SSH sessions through
/// `ssh_jumps` (in client→target order) and exposes a local TCP
/// endpoint on 127.0.0.1 that forwards to `(target_host, target_port)`.
#[derive(Debug)]
pub struct SshTunnel {
    ssh_jumps: Vec<String>,
    ssh_user: String,
    ssh_password: Option<String>,
    ssh_key_path: Option<std::path::PathBuf>,
    ssh_port: u16,
    target_host: String,
    target_port: u16,
    active: bool,
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
            active: false,
        })
    }
}

#[async_trait]
impl Tunnel for SshTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        Err(Error::Connection(
            "SshTunnel::establish not yet implemented".to_string(),
        ))
    }

    async fn close(&mut self) -> Result<()> {
        self.active = false;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rejects_empty_jumps() {
        let err = SshTunnel::new(
            vec![],
            "u".into(),
            None,
            None,
            22,
            "t".into(),
            3306,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Config(_)));
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
}
