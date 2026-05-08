# Tools MCP Phase 2: SSH Tunnel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Phase 1 SSH-tunnel guard with a real `SshTunnel` that supports single- and multi-hop SSH jump hosts, so `--tunnel ssh --ssh-jump host[,host,...]` actually opens MySQL connections through the tunnel.

**Architecture:** Pure-Rust SSH using [`russh`](https://docs.rs/russh) with multi-hop chaining (each subsequent SSH session runs over a `direct-tcpip` channel of the prior session). A local TCP listener on `127.0.0.1:<random>` accepts connections from `mysql_async`, forwards them through the final hop's `direct-tcpip` channel to the target service. Host-key verification accepts any key with a fingerprint warning (Phase 2 simplification).

**Tech Stack:** Rust 2024, tokio, russh (~0.46), the existing `Tunnel`/`Connection` traits.

**Out of scope (Phase 3 or later):**
- Strict host-key checking against `~/.ssh/known_hosts`
- Per-jump auth overrides (different user/password per hop)
- SSH key passphrases
- SSH direct-connection subcommand (`tools-mcp ssh`) and Redis subcommand
- MCP server mode

---

## File Structure

**Created:**
- `src/tunnel/ssh.rs` — `SshTunnel` struct, russh `Client` handler, forwarder task, `Tunnel` impl. Single responsibility: turn an `(ssh_jumps, auth, target)` triple into a local TCP endpoint.

**Modified:**
- `Cargo.toml` — add `russh` dependency.
- `src/config/types.rs` — `TunnelConfig::Ssh.ssh_jump: String` → `ssh_jumps: Vec<String>`; custom serde helper accepting both string and array.
- `src/cli/args.rs` — `SshTunnelArgs.ssh_jump: Option<String>` semantically becomes a comma-separated list (parsed in handler).
- `src/cli/handler.rs` — `cli_to_tunnel_config` splits `--ssh-jump` on commas; `execute_mysql` removes the Phase 1 guard and constructs `SshTunnel` for `TunnelConfig::Ssh`.
- `src/tunnel/mod.rs` — declare and re-export `SshTunnel`.
- `README.md` — drop "SSH tunnel coming soon"; document multi-jump syntax.
- `CLAUDE.md`, `AGENTS.md` — update Phase boundaries section.

---

## Task 1: Add `russh` dependency and scaffold `tunnel::ssh`

**Files:**
- Modify: `Cargo.toml`
- Create: `src/tunnel/ssh.rs`
- Modify: `src/tunnel/mod.rs`

- [ ] **Step 1: Add russh to dependencies**

In `Cargo.toml`, append to `[dependencies]` (alphabetical between `mysql_async` and `serde`):

```toml
russh = "0.46"
```

If 0.46 is not on crates.io at build time, run `cargo add russh` and pin whatever current minor is; document the chosen version in your commit message.

- [ ] **Step 2: Create empty SshTunnel scaffold**

Create `src/tunnel/ssh.rs`:

```rust
use crate::error::{Error, Result};
use crate::tunnel::traits::{Tunnel, TunnelEndpoint};
use async_trait::async_trait;

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
    active: bool,
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
```

- [ ] **Step 3: Wire module**

Replace `src/tunnel/mod.rs` with:

```rust
mod direct;
mod ssh;
mod traits;

pub use direct::DirectTunnel;
pub use ssh::SshTunnel;
pub use traits::{Tunnel, TunnelEndpoint};
```

- [ ] **Step 4: Verify compile + new tests pass**

Run: `cargo test test_new_rejects_empty_jumps test_new_accepts_single_jump`
Expected: PASS (both new tests). `cargo build` clean.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/tunnel/
git commit -m "feat(tunnel): add SshTunnel scaffold with russh dependency

Adds the SshTunnel struct, russh dependency, and tunnel module re-export.
establish() returns 'not yet implemented' so the existing Phase 1 guard
in handler.rs continues to fire; later tasks fill in the real logic.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Migrate `TunnelConfig::Ssh.ssh_jump` to `Vec<String>` with serde flex

**Files:**
- Modify: `src/config/types.rs`
- Modify: `src/cli/handler.rs` (the one place that builds `TunnelConfig::Ssh`)

- [ ] **Step 1: Write tests for new schema**

Append to `src/config/types.rs` test module (or create one if absent):

```rust
#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn test_tunnel_config_ssh_accepts_string_for_jump() {
        let yaml = r#"
type: ssh
ssh_jump: bastion.com
ssh_user: admin
"#;
        let cfg: TunnelConfig = serde_yml::from_str(yaml).unwrap();
        match cfg {
            TunnelConfig::Ssh { ssh_jumps, ssh_user, .. } => {
                assert_eq!(ssh_jumps, vec!["bastion.com".to_string()]);
                assert_eq!(ssh_user, "admin");
            }
            _ => panic!("expected Ssh"),
        }
    }

    #[test]
    fn test_tunnel_config_ssh_accepts_array_for_jump() {
        let yaml = r#"
type: ssh
ssh_jump:
  - bastion1.com
  - bastion2.com
ssh_user: admin
"#;
        let cfg: TunnelConfig = serde_yml::from_str(yaml).unwrap();
        match cfg {
            TunnelConfig::Ssh { ssh_jumps, .. } => {
                assert_eq!(
                    ssh_jumps,
                    vec!["bastion1.com".to_string(), "bastion2.com".to_string()]
                );
            }
            _ => panic!("expected Ssh"),
        }
    }
}
```

- [ ] **Step 2: Run new tests — expect compile failure RED**

Run: `cargo test test_tunnel_config_ssh_accepts_string_for_jump`
Expected: FAIL (field `ssh_jumps` does not exist on `TunnelConfig::Ssh`).

- [ ] **Step 3: Update `TunnelConfig::Ssh` schema**

In `src/config/types.rs`, replace the `Ssh` variant of `TunnelConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TunnelConfig {
    Direct,
    Ssh {
        /// One or more jump hosts in client→target order. YAML/TOML accepts
        /// either a single string (legacy single-hop) or a sequence of strings.
        #[serde(rename = "ssh_jump", deserialize_with = "deserialize_string_or_vec")]
        ssh_jumps: Vec<String>,
        ssh_user: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ssh_password: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ssh_key_path: Option<String>,
        #[serde(default = "default_ssh_port")]
        ssh_port: u16,
    },
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }
    match StringOrVec::deserialize(deserializer)? {
        StringOrVec::String(s) => Ok(vec![s]),
        StringOrVec::Vec(v) => Ok(v),
    }
}
```

(Keep the existing `default_ssh_port` helper.)

- [ ] **Step 4: Run new schema tests — expect PASS GREEN**

Run: `cargo test test_tunnel_config_ssh_accepts_string_for_jump test_tunnel_config_ssh_accepts_array_for_jump`
Expected: PASS.

- [ ] **Step 5: Update the one TunnelConfig::Ssh construction site**

In `src/cli/handler.rs`, locate `cli_to_tunnel_config`'s `TunnelKind::Ssh` arm. Change the construction from:

```rust
let ssh_jump = ssh.ssh_jump.clone().ok_or_else(|| {
    Error::Config("--ssh-jump is required when --tunnel=ssh".to_string())
})?;
// ...
Ok(Some(TunnelConfig::Ssh {
    ssh_jump,
    ssh_user,
    // ...
}))
```

to:

```rust
let raw_jump = ssh.ssh_jump.clone().ok_or_else(|| {
    Error::Config("--ssh-jump is required when --tunnel=ssh".to_string())
})?;
let ssh_jumps: Vec<String> = raw_jump
    .split(',')
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .collect();
if ssh_jumps.is_empty() {
    return Err(Error::Config("--ssh-jump must not be empty".to_string()));
}
let ssh_user = ssh.ssh_user.clone().ok_or_else(|| {
    Error::Config("--ssh-user is required when --tunnel=ssh".to_string())
})?;
Ok(Some(TunnelConfig::Ssh {
    ssh_jumps,
    ssh_user,
    ssh_password: ssh.ssh_password.clone(),
    ssh_key_path: ssh.ssh_key_path.clone(),
    ssh_port: ssh.ssh_port.unwrap_or(22),
}))
```

- [ ] **Step 6: Verify full test suite still passes**

Run: `cargo test`
Expected: all 13 prior tests + 2 new schema tests = 15 pass.

- [ ] **Step 7: Commit**

```bash
git add src/config/types.rs src/cli/handler.rs
git commit -m "feat(config): TunnelConfig::Ssh.ssh_jump accepts list of hops

Schema migrates from \`ssh_jump: String\` to \`ssh_jumps: Vec<String>\`.
The YAML/TOML field name stays \`ssh_jump\` (via #[serde(rename)]) and a
custom deserializer accepts either a single string or an array, so
existing single-hop configs keep working unchanged.

CLI --ssh-jump is now comma-separated (e.g. --ssh-jump=b1.com,b2.com).
Phase 1 guard in execute_mysql still fires; SshTunnel::establish is
implemented in later tasks.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: russh `Client` handler — accept-any host key + fingerprint warning

**Files:**
- Modify: `src/tunnel/ssh.rs`

- [ ] **Step 1: Add russh-keys imports and Client struct**

At the top of `src/tunnel/ssh.rs`, after the existing imports, add:

```rust
use russh::client;
use russh::keys::PublicKey;
use std::sync::Arc;
```

Below the `SshTunnel` struct definition (before `impl SshTunnel`), insert:

```rust
/// russh client handler that accepts any server host key but logs a
/// fingerprint warning to stderr. Phase 2 simplification — Phase 3 will
/// add a strict-checking variant backed by ~/.ssh/known_hosts.
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
        let fingerprint = server_public_key.fingerprint(Default::default());
        eprintln!(
            "warning: accepting unverified host key for {}: {}",
            self.label, fingerprint
        );
        Ok(true)
    }
}
```

(If russh's `Handler::Error` or `check_server_key` signature differs in your installed version, adjust to match — the controller already accepts that the russh API may drift.)

- [ ] **Step 2: Verify compile**

Run: `cargo build`
Expected: clean. (No new tests in this task — host-key acceptance is integration behavior verified manually in Task 7.)

- [ ] **Step 3: Commit**

```bash
git add src/tunnel/ssh.rs
git commit -m "feat(tunnel/ssh): add AcceptAnyHostKey client handler

russh client handler that accepts any server host key and prints the
fingerprint as a warning to stderr (matching openssh's
StrictHostKeyChecking=accept-new ergonomics). A strict known_hosts
variant is deferred to Phase 3.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: SSH authentication helper

**Files:**
- Modify: `src/tunnel/ssh.rs`

- [ ] **Step 1: Add authenticate helper**

In `src/tunnel/ssh.rs`, below `AcceptAnyHostKey`, add:

```rust
/// Authenticate `handle` using key path first (if provided), then
/// password. Returns Err if neither succeeds or neither is supplied.
async fn authenticate(
    handle: &mut client::Handle<AcceptAnyHostKey>,
    user: &str,
    password: Option<&str>,
    key_path: Option<&std::path::Path>,
) -> Result<()> {
    if let Some(path) = key_path {
        // load_secret_key returns russh::keys::PrivateKeyWithHashAlg or PrivateKey
        // depending on the version. Adapt to your installed russh-keys API.
        let key = russh::keys::load_secret_key(path, None).map_err(|e| {
            Error::Connection(format!(
                "failed to load SSH key from '{}': {}",
                path.display(),
                e
            ))
        })?;
        let auth = handle
            .authenticate_publickey(user, std::sync::Arc::new(key))
            .await
            .map_err(|e| Error::Connection(format!("SSH publickey auth failed: {e}")))?;
        if auth.success() {
            return Ok(());
        }
        // fall through to password if provided
    }

    if let Some(pw) = password {
        let auth = handle
            .authenticate_password(user, pw)
            .await
            .map_err(|e| Error::Connection(format!("SSH password auth failed: {e}")))?;
        if auth.success() {
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
```

Notes:
- `authenticate_publickey` and `authenticate_password` return `russh::client::AuthResult` (or `bool` in older russh) — call `.success()` accordingly. If the API differs, adapt minimally.
- Passphrase-protected keys: pass `None` and document the limitation. Phase 3 can add `--ssh-key-passphrase`.

- [ ] **Step 2: Verify compile**

Run: `cargo build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/tunnel/ssh.rs
git commit -m "feat(tunnel/ssh): add authenticate helper

Tries publickey auth first when --ssh-key-path is provided, then falls
back to password. Errors include actionable messages so users know
which credential to provide. Passphrase-protected keys deferred.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Single-hop `establish()` (TCP → SSH → direct-tcpip → local listener)

**Files:**
- Modify: `src/tunnel/ssh.rs`

- [ ] **Step 1: Add forwarder + state + single-hop establish**

In `src/tunnel/ssh.rs`, add the forwarder + connection-chain state to `SshTunnel`. Replace the existing `SshTunnel` struct fields with:

```rust
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
    local_port: u16,
    /// Drop signals all background tasks (listener + each forwarder) to stop.
    shutdown: tokio::sync::watch::Sender<bool>,
    /// JoinHandle for the listener-accept loop. Dropped in close() after
    /// signaling shutdown to make close() bounded-time.
    listener_task: tokio::task::JoinHandle<()>,
    /// SSH session handles, ordered client-to-target. Kept alive so the
    /// channels in the listener task don't drop their parents.
    _sessions: Vec<client::Handle<AcceptAnyHostKey>>,
}
```

Update `SshTunnel::new` to initialize `state: None` (drop the `active: bool` field).

Replace the trait impl with:

```rust
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
        let final_session = sessions.last().expect("chain has at least one session");

        // Bind local listener and capture port BEFORE spawning so we can
        // return the endpoint synchronously.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(Error::Io)?;
        let local_port = listener
            .local_addr()
            .map_err(Error::Io)?
            .port();

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let target_host = self.target_host.clone();
        let target_port = self.target_port;
        let final_handle = final_session.clone();

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
                        let session = final_handle.clone();
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
            local_port,
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
```

Add the helper functions below the impl:

```rust
impl SshTunnel {
    /// Open SSH session(s), one per jump host, chained via direct-tcpip.
    /// Returns the chain in client→target order; the last entry is the
    /// session that should open the final direct-tcpip channel to target.
    async fn build_session_chain(&self) -> Result<Vec<client::Handle<AcceptAnyHostKey>>> {
        // Phase 5: single jump only. Multi-jump implemented in Task 6.
        let jump = &self.ssh_jumps[0];
        let cfg = Arc::new(client::Config::default());
        let handler = AcceptAnyHostKey { label: jump.clone() };

        let mut session = client::connect(cfg, (jump.as_str(), self.ssh_port), handler)
            .await
            .map_err(|e| Error::Connection(format!("SSH connect to {jump} failed: {e}")))?;

        authenticate(
            &mut session,
            &self.ssh_user,
            self.ssh_password.as_deref(),
            self.ssh_key_path.as_deref(),
        )
        .await?;

        if self.ssh_jumps.len() > 1 {
            return Err(Error::Connection(
                "multi-hop SSH tunnel not yet implemented (Task 6)".to_string(),
            ));
        }

        Ok(vec![session])
    }
}

/// Bridge `local_stream` ⟷ direct-tcpip channel from `session` to `target_host:target_port`.
async fn forward_one(
    session: client::Handle<AcceptAnyHostKey>,
    target_host: String,
    target_port: u16,
    mut local_stream: tokio::net::TcpStream,
) -> Result<()> {
    let channel = session
        .channel_open_direct_tcpip(target_host.clone(), target_port as u32, "127.0.0.1", 0)
        .await
        .map_err(|e| {
            Error::Connection(format!(
                "open direct-tcpip to {target_host}:{target_port} failed: {e}"
            ))
        })?;
    let mut channel_stream = channel.into_stream();
    tokio::io::copy_bidirectional(&mut local_stream, &mut channel_stream)
        .await
        .map_err(Error::Io)?;
    Ok(())
}
```

Notes:
- The russh `channel_open_direct_tcpip` argument types may vary across versions: signature is roughly `(host: impl Into<String>, port: u32, originator: impl Into<String>, originator_port: u32)`. Adapt as needed.
- `Channel::into_stream()` produces an `AsyncRead + AsyncWrite` wrapper. If the API name differs (e.g. `make_reader_writer`), adapt.
- `copy_bidirectional` propagates the first half-side error; the forwarder doesn't try to be precise about half-close semantics. For MySQL-over-SSH this is fine.

- [ ] **Step 2: Add a smoke test**

Append to the `tests` module in `src/tunnel/ssh.rs`:

```rust
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
```

- [ ] **Step 3: Verify compile + tests pass**

Run: `cargo test`
Expected: 16 tests pass (15 prior + this new one). `cargo build` clean. The full `establish()` is NOT exercised here — that requires a real SSH server and is Task 7's job.

- [ ] **Step 4: Commit**

```bash
git add src/tunnel/ssh.rs
git commit -m "feat(tunnel/ssh): single-hop establish/close + forwarder

Implements SshTunnel::establish for the single-hop case: TCP-connect to
the jump host, SSH handshake + auth, bind a local TCP listener, and
spawn a forwarder task that bridges each accepted connection to the
target via a direct-tcpip channel.

close() signals shutdown, joins the listener task, and drops the SSH
session(s). Multi-hop is rejected with an explicit error pending Task 6.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Multi-hop session chaining

**Files:**
- Modify: `src/tunnel/ssh.rs`

- [ ] **Step 1: Replace `build_session_chain` with the multi-hop version**

In `src/tunnel/ssh.rs`, replace the entire `build_session_chain` body with:

```rust
async fn build_session_chain(&self) -> Result<Vec<client::Handle<AcceptAnyHostKey>>> {
    let cfg = Arc::new(client::Config::default());
    let mut sessions: Vec<client::Handle<AcceptAnyHostKey>> = Vec::with_capacity(self.ssh_jumps.len());

    // Hop 0: TCP-connect directly.
    let first_jump = &self.ssh_jumps[0];
    let handler = AcceptAnyHostKey { label: first_jump.clone() };
    let mut session = client::connect(
        cfg.clone(),
        (first_jump.as_str(), self.ssh_port),
        handler,
    )
    .await
    .map_err(|e| Error::Connection(format!("SSH connect to {first_jump} failed: {e}")))?;
    authenticate(
        &mut session,
        &self.ssh_user,
        self.ssh_password.as_deref(),
        self.ssh_key_path.as_deref(),
    )
    .await?;
    sessions.push(session);

    // Hop 1..N: each over a direct-tcpip channel of the prior session.
    for next_jump in self.ssh_jumps.iter().skip(1) {
        let prev = sessions.last().expect("at least one session");
        let channel = prev
            .channel_open_direct_tcpip(next_jump.clone(), self.ssh_port as u32, "127.0.0.1", 0)
            .await
            .map_err(|e| {
                Error::Connection(format!(
                    "open direct-tcpip to {next_jump}:{} via prior hop failed: {e}",
                    self.ssh_port
                ))
            })?;
        let stream = channel.into_stream();

        let handler = AcceptAnyHostKey { label: next_jump.clone() };
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
        sessions.push(session);
    }

    Ok(sessions)
}
```

Notes:
- `client::connect_stream(config, stream, handler)` accepts any `AsyncRead + AsyncWrite + Unpin + Send + 'static`. If the function name differs (some russh versions use `connect_via_stream`), adapt.
- All hops share the same `ssh_user`/`ssh_password`/`ssh_key_path`/`ssh_port`. Per-hop overrides are Phase 3.

- [ ] **Step 2: Verify compile**

Run: `cargo build`
Expected: clean. Existing 16 tests still pass: `cargo test`.

- [ ] **Step 3: Commit**

```bash
git add src/tunnel/ssh.rs
git commit -m "feat(tunnel/ssh): multi-hop session chaining via direct-tcpip

Each subsequent jump opens its SSH session over a direct-tcpip channel
of the prior session, so the tunnel walks Client→Bastion1→…→BastionN
before forwarding the local listener traffic to the final target.

All hops share the same --ssh-user/--ssh-password/--ssh-key-path/
--ssh-port; per-hop overrides are Phase 3.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: Wire SshTunnel into handler.rs and remove the Phase 1 guard

**Files:**
- Modify: `src/cli/handler.rs`

- [ ] **Step 1: Update `execute_mysql` to construct the right tunnel**

In `src/cli/handler.rs`, locate `execute_mysql`. Replace the current body — including the Phase 1 `SSH tunnel is not yet implemented` guard — with:

```rust
async fn execute_mysql(query: &str, config: Config) -> Result<()> {
    let host = config
        .host
        .ok_or_else(|| Error::Config("MySQL host is required".to_string()))?;
    let port = config.port.unwrap_or(3306);
    let user = config
        .user
        .ok_or_else(|| Error::Config("MySQL user is required".to_string()))?;

    let tunnel: Box<dyn Tunnel> = match config.tunnel {
        None | Some(TunnelConfig::Direct) => Box::new(DirectTunnel::new(host, port)),
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
                host,
                port,
            )?)
        }
    };

    let mut conn = MySQLConnection::new(tunnel, user, config.password, config.database);
    let exec_result = MySQLExecutor::execute(&mut conn, query).await;
    // Always tear down the tunnel + pool, even on query error.
    let _ = conn.disconnect().await;
    let output = CliFormatter::format(&exec_result?);
    println!("{output}");
    Ok(())
}
```

Add the missing import at the top of the file:

```rust
use crate::tunnel::SshTunnel;
```

(`DirectTunnel` and `Tunnel` are already imported.)

The `let result = ...; conn.disconnect; let result = result?;` shape is intentional: it ensures `disconnect()` runs even when the query fails (closing the SSH tunnel in particular), addressing the connection-leak concern from the Phase 1 review.

- [ ] **Step 2: Build + test**

Run: `cargo test`
Expected: 16 tests pass. The Phase 1 `Error::Config("SSH tunnel is not yet implemented...")` is gone. (No new automated tests — SSH end-to-end is verified manually in Task 8.)

- [ ] **Step 3: Commit**

```bash
git add src/cli/handler.rs
git commit -m "feat(cli): wire SshTunnel into MySQL execution path

Removes the Phase 1 'SSH tunnel is not yet implemented' guard. When
TunnelConfig::Ssh is selected, execute_mysql now constructs an
SshTunnel and hands it to MySQLConnection. DirectTunnel still handles
the no-tunnel/Direct cases.

Disconnect now runs whether or not the query succeeded so the SSH
connection is always torn down (was a Phase 1 review concern).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Manual end-to-end smoke + README/docs update

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`

This task has no code changes; it documents the new capability and lays out the manual smoke procedure.

- [ ] **Step 1: Run the manual end-to-end smoke**

If the developer has access to an SSH bastion + MySQL:

```bash
cargo run -- --tunnel ssh \
  --ssh-jump <bastion> --ssh-user <user> --ssh-password '<pwd>' --ssh-port 22 \
  mysql --host <internal-mysql> --port 3306 --user <db-user> --password '<db-pwd>' \
  'show databases;'
```

Expected: a comfy-table list of databases identical in shape to the Phase 1 direct path. The "warning: accepting unverified host key for <bastion>: ..." line should appear once per hop on stderr.

For multi-hop:

```bash
cargo run -- --tunnel ssh \
  --ssh-jump bastion1.com,bastion2.com --ssh-user <user> --ssh-password '<pwd>' \
  mysql --host <internal-mysql> --user <db-user> --password '<db-pwd>' \
  'show databases;'
```

If no bastion is available, skip and document this as a known coverage gap.

- [ ] **Step 2: Update `README.md` Status section**

In `README.md`, replace the Status section's "Not yet implemented" entry for SSH tunnel — it now works. Update to:

```markdown
## Status

This is the Phase 2 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`)
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
- Direct connection (`--tunnel=direct` or no `--tunnel` flag)
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth (`--ssh-password` / `--ssh-key-path`).
  Host keys are accepted with a fingerprint warning (Phase 3 will add strict checking).

Not yet implemented:
- Redis support
- SSH direct connection (`tools-mcp ssh ...`)
- MCP server mode (running without a subcommand prints a placeholder)
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
```

Add a multi-hop usage example in the MySQL Usage block:

```bash
# Through a single SSH jump
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  mysql --host=mysql.internal --user=root --password=dbpass "SELECT 1"

# Through two SSH jumps
tools-mcp --tunnel=ssh --ssh-jump=bastion1.com,bastion2.com --ssh-user=admin \
  --ssh-key-path=~/.ssh/jump_key \
  mysql --host=mysql.internal --user=root --password=dbpass "SELECT 1"
```

- [ ] **Step 3: Update `CLAUDE.md` and `AGENTS.md`**

In both files, update the Phase boundaries section:

Before:
```markdown
- **SSH tunnel**: parses fine into `TunnelConfig::Ssh` but is rejected at runtime in `cli/handler.rs::execute_mysql` with `Error::Config("SSH tunnel is not yet implemented in Phase 1")`. When Phase 2 lands, replace that guard with the real `SshTunnel` construction.
```

After:
```markdown
- **SSH tunnel**: implemented in Phase 2 via `tunnel::SshTunnel` (russh-based). Single- and multi-hop jumps via comma-separated `--ssh-jump`; password or key auth; host keys accepted with fingerprint warning. Strict known_hosts verification, key passphrases, and per-hop auth are Phase 3.
- **MCP server mode**: triggered when no subcommand is given; `main.rs` prints a placeholder and exits 1. (Unchanged from Phase 1.)
```

Update the module map to add `tunnel::ssh`:
```markdown
| `tunnel::{traits,direct,ssh}` | async `Tunnel` trait; `DirectTunnel` (no tunnel) and `SshTunnel` (russh, single/multi-hop, accept-any host key) |
```

- [ ] **Step 4: Verify clean build + full suite + commit**

Run: `cargo build && cargo test && cargo clippy -- -D warnings`
Expected: all green.

```bash
git add README.md CLAUDE.md AGENTS.md
git commit -m "docs: document SSH tunnel as shipped in Phase 2

Update Status section, add multi-hop usage example, refresh CLAUDE.md
and AGENTS.md so future agents see the SSH gate is gone and the
remaining boundaries (Redis / SSH-direct / MCP / strict host keys /
key passphrases / per-hop auth) are Phase 3+ deferred items.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Phase 2 final verification

**Files:** none (verification only).

- [ ] **Step 1: Full test suite**

Run: `cargo test`
Expected: 16 unit tests + 2 integration tests pass.

- [ ] **Step 2: Lint**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Format**

Run: `cargo fmt --all -- --check`
Expected: clean (or run `cargo fmt --all` to fix).

- [ ] **Step 4: Release build**

Run: `cargo build --release`
Expected: `target/release/tools-mcp` produced, no warnings.

- [ ] **Step 5: Help reflects no behavior change at the surface**

Run: `./target/release/tools-mcp mysql --help`
Expected: identical structure to Phase 1 (Tunnel section still lists the same flags). The user-visible Usage line and `--ssh-*` flags are unchanged.

- [ ] **Step 6 (optional): Phase 2 roll-up commit**

If anything was left unstaged across previous tasks, this is the place to mop up; otherwise skip.

---

## Summary

After Phase 2:

- `tools-mcp --tunnel ssh --ssh-jump=h1[,h2,...] mysql ... "QUERY"` opens a working SSH tunnel and executes the query through it.
- Auth: `--ssh-password` or `--ssh-key-path` (no passphrases).
- Host keys: accept-any with a one-line stderr warning per hop.
- The Phase 1 runtime guard in `execute_mysql` is gone.
- Module surface adds `tunnel::SshTunnel`; everything else is unchanged.
- README, CLAUDE.md, AGENTS.md reflect the new boundary line.

**Deferred to Phase 3+:** Redis subcommand, SSH-direct subcommand, MCP server mode, strict known_hosts checking, SSH key passphrases, per-hop auth overrides.
