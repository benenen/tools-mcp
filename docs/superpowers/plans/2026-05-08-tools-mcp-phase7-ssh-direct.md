# Tools MCP Phase 7: SSH-Direct (Remote Command Execution) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `tools-mcp ssh "<COMMAND>" --host=... --user=... --key-path=...` CLI subcommand and `ssh_exec` MCP tool that runs a shell command on a target SSH server, optionally going through a chain of SSH jump hosts (reusing the existing Phase 2 multi-hop infrastructure).

**Architecture:** Model A — `ssh-direct` is its own service, with a SEPARATE target host/user/auth from the optional jump credentials. The full path is `client → bastion(s) → target SSH server`, where the command runs on TARGET. New `tools-mcp-ssh` lib crate owns the russh `session` channel + `exec` request glue. Refactor: extract `AcceptAnyHostKey` + `authenticate` + `build_session_chain` from the bin's `src/tunnel/ssh.rs` into the new lib so both `SshTunnel` (existing tunnel forwarding) and `SshExec` (new command execution) share the same hop-chaining primitives.

**Phase 7 deliberately defers Profile/YAML support for ssh-direct** — only CLI flags + global `--tunnel`/`--ssh-*` apply. Pattern matches Phase 6 HTTP: orchestrator takes a typed `SshExecRequest` + `Option<TunnelConfig>`, no `Config` plumbing.

**Tech Stack:** russh 0.46 (already a dep), reused via the new lib. Plus shared `tools-mcp-core`.

**Out of scope (Phase 8+):**
- SSH key passphrases (current limitation, inherited from Phase 2).
- Profile/YAML support for ssh-direct.
- Per-jump auth overrides (all jumps still share one credential set; target has its own).
- Strict known_hosts host-key checking (still accept-any with stderr warning).
- Streaming stdout/stderr (we collect everything before returning).
- Subsystem channels (sftp etc.).
- PTY allocation (commands that need a TTY won't work; `top` etc. will fail).

---

## File Structure

**New:**
- `crates/tools-mcp-ssh/Cargo.toml` — async-trait + russh + tokio + tools-mcp-core.
- `crates/tools-mcp-ssh/src/lib.rs` — re-exports.
- `crates/tools-mcp-ssh/src/session.rs` — `AcceptAnyHostKey`, `authenticate`, `build_session_chain` (moved from bin). The Phase 2 helpers, now shared.
- `crates/tools-mcp-ssh/src/request.rs` — `SshExecRequest` (host/port/user/password/key_path/command).
- `crates/tools-mcp-ssh/src/exec.rs` — `SshExec::run(final_session, command_str)` (channel_open_session + exec + collect stdout/stderr/exit) + Response → ExecutionResult mapping.
- `crates/tools-mcp-ssh/src/execute.rs` — `execute(req, jumps_config)` top-level entry: build chain through jumps if any, open ANOTHER SSH session to target with target creds, exec, return ExecutionResult.

**Modified:**
- `Cargo.toml` (workspace) — add `crates/tools-mcp-ssh` to `members`; bin `[dependencies]` gains `tools-mcp-ssh = { path = "crates/tools-mcp-ssh" }`.
- `src/tunnel/ssh.rs` — replace inline `AcceptAnyHostKey` / `authenticate` / `build_session_chain` definitions with imports from `tools_mcp_ssh::session`.
- `src/core/mod.rs` — `pub mod ssh;`.
- `src/core/ssh.rs` — orchestrator `execute(SshExecRequest, Option<TunnelConfig>) -> ExecutionResult`.
- `src/cli/args.rs` — add `Commands::Ssh { command, host, port, user, password, key_path }`.
- `src/cli/handler.rs` — handle the new variant; add `execute_ssh` wrapper.
- `src/mcp/tools.rs` — add `SshExecParams` + `ssh_exec(params)` entry.
- `src/mcp/server.rs` — register `#[tool] ssh_exec`.
- `tests/mcp_smoke.rs` — assert all 4 tools list (`mysql_exec` / `redis_exec` / `http_exec` / `ssh_exec`).
- `commands/ssh.md` — new slash command.
- `skills/ssh-using/SKILL.md` — new skill.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — document Phase 7.

---

## Task 1: Bootstrap empty `tools-mcp-ssh` crate

**Files:**
- Modify: `Cargo.toml` (workspace `members` + bin `[dependencies]`)
- Create: `crates/tools-mcp-ssh/Cargo.toml`
- Create: `crates/tools-mcp-ssh/src/lib.rs` (placeholder)

- [ ] **Step 1: Create `crates/tools-mcp-ssh/Cargo.toml`**

```toml
[package]
name = "tools-mcp-ssh"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
russh = "0.46"
tokio = { version = "1.40", features = ["sync", "macros"] }
tools-mcp-core = { path = "../tools-mcp-core" }

[dev-dependencies]
tokio = { version = "1.40", features = ["macros", "rt-multi-thread"] }
```

`russh = "0.46"` matches the version the bin already uses. `tokio` minimal feature set covers `Mutex` + the test macros.

- [ ] **Step 2: Create `crates/tools-mcp-ssh/src/lib.rs`**

```rust
//! SSH session-chain primitives (shared by `tools-mcp` bin's SshTunnel and
//! this crate's SshExec) plus a top-level `execute()` function for running
//! a single shell command on an SSH target, optionally through one or more
//! jump hosts.
```

- [ ] **Step 3: Wire workspace members and bin dep**

In root `Cargo.toml`:

a) Update `[workspace] members` (alphabetical):

```toml
[workspace]
resolver = "3"
members = [
    "crates/tools-mcp-core",
    "crates/tools-mcp-http",
    "crates/tools-mcp-mysql",
    "crates/tools-mcp-redis",
    "crates/tools-mcp-ssh",
]
```

b) Update bin `[dependencies]` — add (alphabetical, between `tools-mcp-redis` and `toml`):

```toml
tools-mcp-ssh = { path = "crates/tools-mcp-ssh" }
```

- [ ] **Step 4: Verify**

Run: `cargo build`
Expected: clean. New crate is empty; nothing breaks.

Run: `cargo test`
Expected: prior count passes (no regressions).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ssh): scaffold tools-mcp-ssh lib crate

Empty crate skeleton with russh + tools-mcp-core deps. Subsequent
tasks extract the SSH session-chain primitives from the bin's
SshTunnel into this lib (so both SshTunnel and the new SshExec share
build_session_chain), and add SshExec on top.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Refactor — move `AcceptAnyHostKey` / `authenticate` / `build_session_chain` from bin to `tools-mcp-ssh::session`

**Files:**
- Create: `crates/tools-mcp-ssh/src/session.rs`
- Modify: `crates/tools-mcp-ssh/src/lib.rs` (declare + re-export)
- Modify: `src/tunnel/ssh.rs` (delete inline copies, import from lib)

This is a pure refactor — no behavior change. The 18 prior tests + Phase 2's manual smoke tests must continue to pass after the move.

The existing `src/tunnel/ssh.rs` contains:

- `struct AcceptAnyHostKey { label: String }` + `impl client::Handler` — line 38ish onward
- `async fn authenticate(handle, user, password, key_path) -> Result<()>` — line 66 onward
- `async fn SshTunnel::build_session_chain(&self) -> Result<Vec<Arc<Mutex<...>>>>` — line 224 onward (an inherent method on `SshTunnel`)

The refactor: move the first two as-is to the new lib, and convert `build_session_chain` from an `&self` method to a free function taking explicit `(jumps, user, password, key_path, port)` parameters, since `SshExec` doesn't have an `SshTunnel` to call it on.

- [ ] **Step 1: Create `crates/tools-mcp-ssh/src/session.rs`**

Read `src/tunnel/ssh.rs` to find the EXACT current bodies of `AcceptAnyHostKey`, `impl client::Handler for AcceptAnyHostKey`, `authenticate()`, and `SshTunnel::build_session_chain()`. The text below shows the SHAPE; keep field/variable names and message strings byte-identical to the originals so behavior doesn't drift.

Create `crates/tools-mcp-ssh/src/session.rs`:

```rust
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
/// StrictHostKeyChecking=accept-new ergonomics).
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
pub async fn authenticate(
    handle: &mut client::Handle<AcceptAnyHostKey>,
    user: &str,
    password: Option<&str>,
    key_path: Option<&std::path::Path>,
) -> Result<()> {
    if let Some(path) = key_path {
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
```

(If the bin's existing `build_session_chain` body differs slightly — e.g. variable names, error messages, parameter order — adapt to match the original byte-for-byte. The point is to MOVE working code, not rewrite it.)

- [ ] **Step 2: Update `crates/tools-mcp-ssh/src/lib.rs`**

```rust
//! SSH session-chain primitives (shared by `tools-mcp` bin's SshTunnel and
//! this crate's SshExec) plus a top-level `execute()` function for running
//! a single shell command on an SSH target, optionally through one or more
//! jump hosts.

pub mod session;

pub use session::{AcceptAnyHostKey, authenticate, build_session_chain};
```

- [ ] **Step 3: Refactor `src/tunnel/ssh.rs` to import from the lib**

In `src/tunnel/ssh.rs`:

a) DELETE these from the file (they now live in `tools_mcp_ssh::session`):
- `struct AcceptAnyHostKey { ... }` + its `#[async_trait] impl client::Handler` block
- `async fn authenticate(...)` (top-level free function)
- `impl SshTunnel { async fn build_session_chain(&self) ... }` (the inherent method)

b) ADD this import at the top, alongside the existing `use russh::client;` etc.:

```rust
use tools_mcp_ssh::session::{authenticate, build_session_chain, AcceptAnyHostKey};
```

c) UPDATE the call site in `SshTunnel::establish` (and anywhere else) that currently calls `self.build_session_chain()`. It now becomes:

```rust
let sessions = build_session_chain(
    &self.ssh_jumps,
    &self.ssh_user,
    self.ssh_password.as_deref(),
    self.ssh_key_path.as_deref(),
    self.ssh_port,
)
.await?;
```

(The struct fields `ssh_jumps`, `ssh_user`, `ssh_password`, `ssh_key_path`, `ssh_port` are unchanged.)

d) The `_sessions: Vec<Arc<Mutex<client::Handle<AcceptAnyHostKey>>>>` field type in `SshTunnelState` continues to compile because `AcceptAnyHostKey` is now imported.

- [ ] **Step 4: Verify the refactor preserves behavior**

Run: `cargo build`
Expected: clean. If anything's missing (e.g. you forgot to import `AcceptAnyHostKey`), the compiler will tell you.

Run: `cargo test`
Expected: all prior tests pass (the SshTunnel tests in particular — they construct an SshTunnel and check `is_active()`, no real SSH involved).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

(Manual smoke against a real bastion is optional here; it'll be covered in Task 9.)

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(ssh): move session-chain primitives into tools-mcp-ssh lib

AcceptAnyHostKey, authenticate(), and build_session_chain are now in
crates/tools-mcp-ssh/src/session.rs. The bin's SshTunnel imports them
from there. build_session_chain is now a free function (taking jumps/
user/password/key_path/port explicitly) instead of an inherent method
on SshTunnel — that lets the upcoming SshExec reuse it without going
through SshTunnel.

Pure refactor; no behavior change. All 30+ tests still pass.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `SshExecRequest` type

**Files:**
- Create: `crates/tools-mcp-ssh/src/request.rs`
- Modify: `crates/tools-mcp-ssh/src/lib.rs`

- [ ] **Step 1: Define the request type**

Create `crates/tools-mcp-ssh/src/request.rs`:

```rust
//! SSH-direct request input shape — independent of any caller (CLI, MCP).

/// Resolved SSH-exec request to execute. Caller (CLI handler / MCP tool)
/// builds this from the user's flags / JSON params; the lib doesn't care
/// where the fields came from.
#[derive(Debug, Clone)]
pub struct SshExecRequest {
    /// Target SSH host (the machine where `command` runs).
    pub host: String,
    /// Target SSH port (default 22).
    pub port: u16,
    /// Target SSH user.
    pub user: String,
    /// Target SSH password (mutually exclusive with key_path; at least one
    /// of password / key_path must be provided).
    pub password: Option<String>,
    /// Path to an unencrypted private key file (passphrase-protected keys
    /// are not supported in Phase 7).
    pub key_path: Option<std::path::PathBuf>,
    /// Shell command to execute on the target.
    pub command: String,
}
```

- [ ] **Step 2: Update `crates/tools-mcp-ssh/src/lib.rs`**

```rust
//! SSH session-chain primitives (shared by `tools-mcp` bin's SshTunnel and
//! this crate's SshExec) plus a top-level `execute()` function for running
//! a single shell command on an SSH target, optionally through one or more
//! jump hosts.

pub mod request;
pub mod session;

pub use request::SshExecRequest;
pub use session::{AcceptAnyHostKey, authenticate, build_session_chain};
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo test`
Expected: prior count passes (no new tests yet).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ssh): SshExecRequest input type

Plain data type describing the SSH-direct request. Caller (CLI handler /
MCP tool) builds it from their respective input shapes; the lib doesn't
care where fields came from. host/port/user are required at the type
level; password OR key_path is required at runtime (validated in the
authenticate() helper).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `SshExec::run` (channel exec + collect output) + `Result → ExecutionResult` mapping

**Files:**
- Create: `crates/tools-mcp-ssh/src/exec.rs`
- Modify: `crates/tools-mcp-ssh/src/lib.rs`

- [ ] **Step 1: Write the executor**

Create `crates/tools-mcp-ssh/src/exec.rs`:

```rust
//! Open a session channel on a russh client, exec a command, collect
//! stdout/stderr/exit_code, and map into an `ExecutionResult`.

use russh::ChannelMsg;
use russh::client;
use std::sync::Arc;
use tokio::sync::Mutex;
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::session::AcceptAnyHostKey;

pub struct SshExec;

/// Stdout/stderr collected during exec, plus the remote exit code (if the
/// remote sent one — for clean exits this is always `Some`).
#[derive(Debug, Clone)]
pub struct SshOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<u32>,
}

impl SshExec {
    /// Open a `session` channel on `final_session`, exec `command`, and
    /// collect stdout/stderr/exit_code until the channel closes.
    pub async fn run(
        final_session: Arc<Mutex<client::Handle<AcceptAnyHostKey>>>,
        command: &str,
    ) -> Result<SshOutput> {
        let mut channel = final_session
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(|e| Error::Service(format!("SSH session open failed: {e}")))?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| Error::Service(format!("SSH exec request failed: {e}")))?;

        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let mut exit_code: Option<u32> = None;

        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.extend_from_slice(data);
                }
                ChannelMsg::ExtendedData { ref data, ext } if ext == 1 => {
                    stderr.extend_from_slice(data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(exit_status);
                }
                _ => {}
            }
        }

        Ok(SshOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
}

/// Map collected SSH output into an `ExecutionResult` with rows
/// `["exit_code", ...]`, `["stdout", ...]`, `["stderr", ...]`.
/// Bytes are UTF-8-decoded if possible; otherwise rendered as
/// `<N bytes (non-UTF-8)>`.
pub fn output_to_result(output: SshOutput) -> ExecutionResult {
    let stdout_cell = bytes_to_cell(&output.stdout);
    let stderr_cell = bytes_to_cell(&output.stderr);
    let exit_cell = match output.exit_code {
        Some(c) => c.to_string(),
        None => "<unknown>".to_string(),
    };

    let rows: Vec<Vec<String>> = vec![
        vec!["exit_code".to_string(), exit_cell],
        vec!["stdout".to_string(), stdout_cell],
        vec!["stderr".to_string(), stderr_cell],
    ];
    let affected = rows.len() as u64;
    ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        rows,
        affected,
    )
}

fn bytes_to_cell(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(text) => text.to_string(),
        Err(_) => format!("<{} bytes (non-UTF-8)>", b.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_to_result_utf8() {
        let out = SshOutput {
            stdout: b"hello\n".to_vec(),
            stderr: b"warn: something\n".to_vec(),
            exit_code: Some(0),
        };
        let r = output_to_result(out);
        assert_eq!(r.columns, vec!["field".to_string(), "value".to_string()]);
        assert_eq!(r.affected_rows, 3);
        assert_eq!(r.rows[0], vec!["exit_code".to_string(), "0".to_string()]);
        assert_eq!(r.rows[1], vec!["stdout".to_string(), "hello\n".to_string()]);
        assert_eq!(
            r.rows[2],
            vec!["stderr".to_string(), "warn: something\n".to_string()]
        );
    }

    #[test]
    fn test_output_to_result_non_utf8() {
        let out = SshOutput {
            stdout: vec![0xff, 0xfe, 0xfd],
            stderr: Vec::new(),
            exit_code: Some(127),
        };
        let r = output_to_result(out);
        assert_eq!(r.rows[0], vec!["exit_code".to_string(), "127".to_string()]);
        assert_eq!(
            r.rows[1],
            vec!["stdout".to_string(), "<3 bytes (non-UTF-8)>".to_string()]
        );
    }

    #[test]
    fn test_output_to_result_unknown_exit() {
        let out = SshOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: None,
        };
        let r = output_to_result(out);
        assert_eq!(
            r.rows[0],
            vec!["exit_code".to_string(), "<unknown>".to_string()]
        );
    }
}
```

- [ ] **Step 2: Update `crates/tools-mcp-ssh/src/lib.rs`**

```rust
//! SSH session-chain primitives (shared by `tools-mcp` bin's SshTunnel and
//! this crate's SshExec) plus a top-level `execute()` function for running
//! a single shell command on an SSH target, optionally through one or more
//! jump hosts.

pub mod exec;
pub mod request;
pub mod session;

pub use exec::{SshExec, SshOutput, output_to_result};
pub use request::SshExecRequest;
pub use session::{AcceptAnyHostKey, authenticate, build_session_chain};
```

- [ ] **Step 3: Verify**

Run: `cargo test --package tools-mcp-ssh`
Expected: 3 PASS (the three `output_to_result` tests).

Run: `cargo test`
Expected: prior count + 3 new = pass.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ssh): SshExec + output → ExecutionResult mapping

SshExec::run opens a session channel on the supplied russh handle,
sends the exec request, and drains stdout / extended-data (stderr) /
exit_status messages until the channel closes. Returns SshOutput
(raw bytes + exit code).

output_to_result maps SshOutput into an ExecutionResult with rows
[exit_code, stdout, stderr]. UTF-8 decode for text bodies; fallback
'<N bytes (non-UTF-8)>' for binary. Missing exit code (channel closed
without ExitStatus) renders as '<unknown>'.

3 unit tests cover utf8 / non-utf8 / unknown-exit paths.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: `tools_mcp_ssh::execute(req, jumps_config)` entry function

**Files:**
- Create: `crates/tools-mcp-ssh/src/execute.rs`
- Modify: `crates/tools-mcp-ssh/src/lib.rs`

This task glues the pieces: build the optional jump chain, open the FINAL SSH session to the target with target creds, run the command.

- [ ] **Step 1: Define the optional jumps shape and write `execute`**

The orchestrator (Task 6) needs to pass jump info into the lib. Define a small struct for that:

Append to `crates/tools-mcp-ssh/src/request.rs`:

```rust
/// Optional SSH-jump config: a chain of bastion hosts plus the credentials
/// to authenticate to ALL of them (per-hop overrides aren't supported yet).
/// When `None` is passed to `execute`, the target SSH server is reached
/// directly.
#[derive(Debug, Clone)]
pub struct SshJumpsConfig {
    pub jumps: Vec<String>,
    pub user: String,
    pub password: Option<String>,
    pub key_path: Option<std::path::PathBuf>,
    pub port: u16,
}
```

Then create `crates/tools-mcp-ssh/src/execute.rs`:

```rust
//! Top-level entry: build (optional) SSH jump chain, open final SSH
//! session to the target with target credentials, exec the command,
//! map the output to an ExecutionResult.

use russh::client;
use std::sync::Arc;
use tokio::sync::Mutex;
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::exec::{SshExec, output_to_result};
use crate::request::{SshExecRequest, SshJumpsConfig};
use crate::session::{AcceptAnyHostKey, authenticate, build_session_chain};

/// Run a single shell command on the SSH target described by `req`,
/// optionally going through `jumps`. Always tears down the chain via Drop
/// before returning.
pub async fn execute(
    req: SshExecRequest,
    jumps: Option<SshJumpsConfig>,
) -> Result<ExecutionResult> {
    let cfg = std::sync::Arc::new(client::Config::default());

    // Build the jump chain (if any). Returns the last jump's session.
    let mut jump_sessions = match &jumps {
        Some(j) if !j.jumps.is_empty() => {
            build_session_chain(
                &j.jumps,
                &j.user,
                j.password.as_deref(),
                j.key_path.as_deref(),
                j.port,
            )
            .await?
        }
        _ => Vec::new(),
    };

    // Open the FINAL SSH session to the target. If we have a jump chain,
    // open a direct-tcpip channel from the last jump and run SSH over it
    // (with TARGET's credentials, not the jump credentials). If we don't,
    // TCP-connect directly.
    let target_handler = AcceptAnyHostKey {
        label: req.host.clone(),
    };
    let mut target_session = if let Some(last_jump) = jump_sessions.last() {
        let channel = last_jump
            .lock()
            .await
            .channel_open_direct_tcpip(req.host.clone(), req.port as u32, "127.0.0.1", 0u32)
            .await
            .map_err(|e| {
                Error::Connection(format!(
                    "open direct-tcpip to {}:{} via last jump failed: {e}",
                    req.host, req.port
                ))
            })?;
        let stream = Box::pin(channel.into_stream());
        client::connect_stream(cfg, stream, target_handler)
            .await
            .map_err(|e| {
                Error::Connection(format!(
                    "SSH connect to {} (chained) failed: {e}",
                    req.host
                ))
            })?
    } else {
        client::connect(cfg, (req.host.as_str(), req.port), target_handler)
            .await
            .map_err(|e| Error::Connection(format!("SSH connect to {} failed: {e}", req.host)))?
    };

    // Authenticate with TARGET's creds (not the jump creds).
    authenticate(
        &mut target_session,
        &req.user,
        req.password.as_deref(),
        req.key_path.as_deref(),
    )
    .await?;

    let target_session = Arc::new(Mutex::new(target_session));

    // Exec the command.
    let result = SshExec::run(target_session.clone(), &req.command).await;

    // Drop the target session and the jump chain (Drop closes the
    // underlying channels/connections).
    drop(target_session);
    jump_sessions.clear();

    Ok(output_to_result(result?))
}
```

- [ ] **Step 2: Update `crates/tools-mcp-ssh/src/lib.rs`**

```rust
//! SSH session-chain primitives (shared by `tools-mcp` bin's SshTunnel and
//! this crate's SshExec) plus a top-level `execute()` function for running
//! a single shell command on an SSH target, optionally through one or more
//! jump hosts.

pub mod exec;
pub mod execute;
pub mod request;
pub mod session;

pub use exec::{SshExec, SshOutput, output_to_result};
pub use execute::execute;
pub use request::{SshExecRequest, SshJumpsConfig};
pub use session::{AcceptAnyHostKey, authenticate, build_session_chain};
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo test`
Expected: prior + 0 new (no tests in this task; `execute` requires a real SSH server).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ssh): execute(req, jumps) entry function

When jumps is Some with at least one host: build the jump chain via
shared build_session_chain, then open one MORE SSH session over a
direct-tcpip channel of the last jump — that final session
authenticates with TARGET's credentials (not jump creds). Otherwise
TCP-connect directly to the target.

After auth, run the command via SshExec::run, map to ExecutionResult
via output_to_result, and drop all sessions (Drop closes the
underlying transport).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Bin orchestrator `core::ssh::execute`

**Files:**
- Create: `src/core/ssh.rs`
- Modify: `src/core/mod.rs`

- [ ] **Step 1: Write the orchestrator**

Create `src/core/ssh.rs`:

```rust
//! Orchestrator: take a typed SshExecRequest + an optional TunnelConfig,
//! translate to tools_mcp_ssh's request shape, dispatch into the lib.
//! CLI handler and MCP `ssh_exec` tool both delegate here.

use crate::config::TunnelConfig;
use tools_mcp_core::{Error, ExecutionResult, Result};
use tools_mcp_ssh::{SshExecRequest, SshJumpsConfig, execute as ssh_execute};

pub async fn execute(
    req: SshExecRequest,
    tunnel_config: Option<TunnelConfig>,
) -> Result<ExecutionResult> {
    if req.password.is_none() && req.key_path.is_none() {
        return Err(Error::Config(
            "SSH target requires --password or --key-path".to_string(),
        ));
    }

    let jumps = match tunnel_config {
        None | Some(TunnelConfig::Direct) => None,
        Some(TunnelConfig::Ssh {
            ssh_jumps,
            ssh_user,
            ssh_password,
            ssh_key_path,
            ssh_port,
        }) => Some(SshJumpsConfig {
            jumps: ssh_jumps,
            user: ssh_user,
            password: ssh_password,
            key_path: ssh_key_path.map(std::path::PathBuf::from),
            port: ssh_port,
        }),
    };

    ssh_execute(req, jumps).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_req() -> SshExecRequest {
        SshExecRequest {
            host: "h".to_string(),
            port: 22,
            user: "u".to_string(),
            password: None,
            key_path: None,
            command: "ls".to_string(),
        }
    }

    #[tokio::test]
    async fn test_execute_errors_without_password_or_key() {
        let err = execute(empty_req(), None).await.unwrap_err();
        assert!(
            matches!(err, Error::Config(msg) if msg.contains("--password or --key-path")),
            "expected Config error about missing creds"
        );
    }
}
```

- [ ] **Step 2: Wire `core::ssh` into `src/core/mod.rs`**

Update `src/core/mod.rs` from:

```rust
pub mod http;
pub mod mysql;
pub mod redis;
```

to:

```rust
pub mod http;
pub mod mysql;
pub mod redis;
pub mod ssh;
```

- [ ] **Step 3: Verify**

Run: `cargo test test_execute_errors_without_password_or_key`
Expected: 1 PASS.

Run: `cargo test`
Expected: prior + 1 new = pass.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core): add core::ssh::execute orchestrator

Symmetric to core::http::execute (also no Profile/YAML in Phase 7):
takes (SshExecRequest, Option<TunnelConfig>), translates the optional
SSH tunnel into SshJumpsConfig, dispatches into tools_mcp_ssh.

Validates that the target has at least one credential (password or
key_path) before delegating — surfaces the config error early instead
of waiting for SSH-level rejection.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: CLI subcommand `tools-mcp ssh "<COMMAND>"`

**Files:**
- Modify: `src/cli/args.rs` (add `Commands::Ssh`)
- Modify: `src/cli/handler.rs` (handle the new variant; add `execute_ssh`)

- [ ] **Step 1: Add the `Ssh` variant**

In `src/cli/args.rs`, append a new variant to the `Commands` enum AFTER the existing `Mysql` / `Redis` / `Http` variants:

```rust
    /// Execute a shell command on an SSH target
    #[command(override_usage = "tools-mcp [GLOBAL OPTIONS] ssh [OPTIONS] <COMMAND>")]
    #[command(after_help = USAGE_LEGEND)]
    Ssh {
        /// Shell command to execute on the target.
        command: String,

        /// Target SSH host.
        #[arg(long, help_heading = "SSH")]
        host: String,

        /// Target SSH port (default 22).
        #[arg(long, help_heading = "SSH", default_value_t = 22)]
        port: u16,

        /// Target SSH user.
        #[arg(long, help_heading = "SSH")]
        user: String,

        /// Target SSH password (mutually exclusive with --key-path).
        #[arg(long, help_heading = "SSH", conflicts_with = "key_path")]
        password: Option<String>,

        /// Target SSH key path. Unencrypted keys only (passphrases not
        /// supported in this phase).
        #[arg(long = "key-path", help_heading = "SSH", conflicts_with = "password")]
        key_path: Option<std::path::PathBuf>,

        /// Print full ExecutionResult table (exit_code + stdout + stderr)
        /// instead of streaming stdout/stderr to the terminal. Default:
        /// stream stdout to stdout, stderr to stderr, exit with the
        /// remote exit code.
        #[arg(long = "include-headers", short = 'i', help_heading = "SSH")]
        include_headers: bool,
    },
```

Note: `host` and `user` are `String` (NOT `Option<String>`) — clap requires them at parse time. `port` defaults to 22.

- [ ] **Step 2: Wire the handler**

In `src/cli/handler.rs`, add a new arm to the `match cli.command.clone()` block in `handle()`. Place it after `Http` and before `None`:

```rust
    Some(Commands::Ssh {
        command,
        host,
        port,
        user,
        password,
        key_path,
        include_headers,
    }) => {
        Self::execute_ssh(
            &cli,
            command,
            host,
            port,
            user,
            password,
            key_path,
            include_headers,
        )
        .await
    }
```

Then append a new `execute_ssh` method to `impl CliHandler`:

```rust
    async fn execute_ssh(
        cli: &Cli,
        command: String,
        host: String,
        port: u16,
        user: String,
        password: Option<String>,
        key_path: Option<std::path::PathBuf>,
        include_headers: bool,
    ) -> Result<()> {
        let req = tools_mcp_ssh::SshExecRequest {
            host,
            port,
            user,
            password,
            key_path,
            command,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result = crate::core::ssh::execute(req, tunnel_config).await?;

        if include_headers {
            println!("{}", CliFormatter::format(&result));
            return Ok(());
        }

        // Default: print stdout to stdout, stderr to stderr, exit with the
        // remote exit code.
        let mut exit_code: i32 = 0;
        for row in &result.rows {
            if row.len() < 2 {
                continue;
            }
            match row[0].as_str() {
                "exit_code" => {
                    exit_code = row[1].parse().unwrap_or(0);
                }
                "stdout" => {
                    use std::io::Write;
                    let _ = std::io::stdout().write_all(row[1].as_bytes());
                }
                "stderr" => {
                    use std::io::Write;
                    let _ = std::io::stderr().write_all(row[1].as_bytes());
                }
                _ => {}
            }
        }
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        Ok(())
    }
```

The `std::process::exit(exit_code)` mirrors `ssh user@host "cmd"` behavior — non-zero remote exit propagates to the local shell. With `--include-headers`, we just print the table and let main.rs exit normally (0).

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo run -q -- ssh --help 2>&1 | head -25`
Expected output includes:

```
Execute a shell command on an SSH target

Usage: tools-mcp [GLOBAL OPTIONS] ssh [OPTIONS] <COMMAND>

Arguments:
  <COMMAND>  Shell command to execute on the target

Options:
      --config <CONFIG>  Path to YAML config file
  -h, --help             Print help

SSH:
      --host <HOST>          Target SSH host
      --port <PORT>          Target SSH port (default 22) [default: 22]
      --user <USER>          Target SSH user
      --password <PASSWORD>  Target SSH password (mutually exclusive ...
      --key-path <KEY_PATH>  Target SSH key path. Unencrypted keys only ...
  -i, --include-headers      Print full ExecutionResult table ...

Tunnel:
      --tunnel <TUNNEL>      ...
```

Run: `cargo run -q -- ssh "ls" 2>&1 | head -3`
Expected: clap error about missing required `--host` and `--user` (since they're not Option).

Run: `cargo test`
Expected: prior count passes (no new tests).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(cli): add 'ssh <COMMAND>' subcommand

CLI: tools-mcp ssh \"ls -la\" --host=server --user=admin
       --key-path=~/.ssh/id_rsa
     [--tunnel=ssh --ssh-jump=bastion ... for jump hosts]
     [-i to print exit_code/stdout/stderr table instead of streaming]

Default mode mirrors openssh behavior: stdout streams to stdout,
stderr to stderr, exit code of tools-mcp matches remote command's
exit code (so 'tools-mcp ssh \"test -f /etc/passwd\"' is shell-script
friendly).

--include-headers / -i opts into the structured ExecutionResult table
view (useful for debugging).

Phase 7 doesn't apply Profile/YAML merging for ssh — only CLI flags
+ global tunnel.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: MCP `ssh_exec` tool

**Files:**
- Modify: `src/mcp/tools.rs` (add `SshExecParams` + `ssh_exec` entry)
- Modify: `src/mcp/server.rs` (register the tool)
- Modify: `tests/mcp_smoke.rs` (assert all four tools list)

- [ ] **Step 1: Add `SshExecParams` + entry function in `src/mcp/tools.rs`**

Append to `src/mcp/tools.rs`:

```rust
/// JSON parameters for the `ssh_exec` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SshExecParams {
    /// Shell command to execute on the target.
    pub command: String,

    /// Target SSH host.
    pub host: String,

    /// Target SSH port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Target SSH user.
    pub user: String,

    /// Target SSH password (mutually exclusive with key_path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Target SSH key path. Unencrypted keys only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH jump key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn ssh_params_to_request_and_tunnel(
    p: SshExecParams,
) -> Result<(tools_mcp_ssh::SshExecRequest, Option<TunnelConfig>)> {
    if p.password.is_some() && p.key_path.is_some() {
        return Err(Error::Config(
            "password and key_path are mutually exclusive".to_string(),
        ));
    }

    let req = tools_mcp_ssh::SshExecRequest {
        host: p.host,
        port: p.port.unwrap_or(22),
        user: p.user,
        password: p.password,
        key_path: p.key_path.map(std::path::PathBuf::from),
        command: p.command,
    };

    let tunnel_config = build_tunnel_config_for_ssh_direct(
        p.tunnel,
        p.ssh_jump,
        p.ssh_user,
        p.ssh_password,
        p.ssh_key_path,
        p.ssh_port,
    )?;

    Ok((req, tunnel_config))
}

fn build_tunnel_config_for_ssh_direct(
    kind: Option<TunnelKind>,
    ssh_jump: Option<SshJumpInput>,
    ssh_user: Option<String>,
    ssh_password: Option<String>,
    ssh_key_path: Option<String>,
    ssh_port: Option<u16>,
) -> Result<Option<TunnelConfig>> {
    let Some(kind) = kind else { return Ok(None); };
    match kind {
        TunnelKind::Direct => {
            let stray = ssh_jump.is_some()
                || ssh_user.is_some()
                || ssh_password.is_some()
                || ssh_key_path.is_some()
                || ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = ssh_jump.map(SshJumpInput::into_jumps).ok_or_else(|| {
                Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
            })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = ssh_user.ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port: ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the ssh_exec tool.
pub async fn ssh_exec(params: SshExecParams) -> Result<ExecutionResult> {
    let (req, tunnel_config) = ssh_params_to_request_and_tunnel(params)?;
    crate::core::ssh::execute(req, tunnel_config).await
}
```

(`build_tunnel_config_for_ssh_direct` is the FOURTH near-identical sibling — mysql/redis/http/ssh-direct all have one. Phase 8 cleanup work to extract a shared helper, deferred per the same logic as Phases 5/6.)

Add a unit test inside the existing `mod tests {}`:

```rust
    #[test]
    fn test_ssh_params_to_request_basic() {
        let p = SshExecParams {
            command: "uptime".into(),
            host: "server.com".into(),
            port: None,
            user: "admin".into(),
            password: Some("pwd".into()),
            key_path: None,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let (req, tunnel) = ssh_params_to_request_and_tunnel(p).unwrap();
        assert_eq!(req.command, "uptime");
        assert_eq!(req.host, "server.com");
        assert_eq!(req.port, 22);
        assert_eq!(req.user, "admin");
        assert_eq!(req.password.as_deref(), Some("pwd"));
        assert!(req.key_path.is_none());
        assert!(tunnel.is_none());
    }

    #[test]
    fn test_ssh_params_password_and_key_mutex() {
        let p = SshExecParams {
            command: "ls".into(),
            host: "h".into(),
            port: None,
            user: "u".into(),
            password: Some("pwd".into()),
            key_path: Some("/k".into()),
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let err = ssh_params_to_request_and_tunnel(p).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("mutually exclusive")));
    }
```

- [ ] **Step 2: Register the tool in `src/mcp/server.rs`**

Append to the existing `impl ToolsMcpServer` block:

```rust
    /// Run a shell command on an SSH target, optionally through SSH jumps.
    #[tool(description = "Execute a shell command on a remote SSH server. Returns exit_code, stdout, and stderr. Optionally route through one or more SSH jump hosts; jump credentials and target credentials are independent.")]
    async fn ssh_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::SshExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::ssh_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json),
                ]))
            }
            Err(e) => Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(e.to_string()),
            ])),
        }
    }
```

- [ ] **Step 3: Update `tests/mcp_smoke.rs` for the fourth tool**

Find the existing `found_mysql` / `found_redis` / `found_http` block and add `found_ssh`:

```rust
    let mut found_mysql = false;
    let mut found_redis = false;
    let mut found_http = false;
    let mut found_ssh = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        if n == 0 {
            break;
        }
        if line.contains("\"id\":2") {
            if line.contains("mysql_exec") {
                found_mysql = true;
            }
            if line.contains("redis_exec") {
                found_redis = true;
            }
            if line.contains("http_exec") {
                found_http = true;
            }
            if line.contains("ssh_exec") {
                found_ssh = true;
            }
            break;
        }
    }
```

Update the assertion block:

```rust
    assert!(found_mysql, "tools/list missing mysql_exec");
    assert!(found_redis, "tools/list missing redis_exec");
    assert!(found_http, "tools/list missing http_exec");
    assert!(found_ssh, "tools/list missing ssh_exec");
```

- [ ] **Step 4: Verify**

Run: `cargo test`
Expected: prior + 2 new (`test_ssh_params_to_request_basic`, `test_ssh_params_password_and_key_mutex`); mcp_smoke now asserts all 4 tools.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(mcp): ssh_exec tool registered on the rmcp server

ToolsMcpServer now exposes mysql_exec / redis_exec / http_exec /
ssh_exec. Same shape across all four: JSON params -> typed request +
tunnel config -> core::<svc>::execute -> ExecutionResult JSON.

mcp_smoke integration test asserts all four tools show up in
tools/list.

build_tunnel_config_for_ssh_direct is the fourth near-identical
sibling builder; extraction of a shared helper deferred to a future
cleanup pass.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Plugin assets — `/ssh` slash command + `ssh-using` skill

**Files:**
- Create: `commands/ssh.md`
- Create: `skills/ssh-using/SKILL.md`

- [ ] **Step 1: `/ssh` slash command**

Create `commands/ssh.md`:

```markdown
---
name: ssh
description: Run a shell command on a remote SSH target through the tools-mcp `ssh_exec` MCP tool, optionally via SSH jump hosts.
argument-hint: "<COMMAND>" --host=... --user=... [--key-path=... | --password=...]
---

# /ssh

Run this shell command on the SSH target via the `ssh_exec` MCP tool:

```
$ARGUMENTS
```

## How to call it

1. **Parse the user's input.** First token is the command (often quoted).
   Required flags: `--host`, `--user`, and either `--password` or
   `--key-path`. Common shapes:
   - `/ssh "ls -la /var/log" --host=server.com --user=admin --key-path=~/.ssh/id_rsa`
   - `/ssh "df -h" --host=10.0.0.5 --user=root --password=...`
   - `/ssh "uname -a" --host=internal --user=admin --key-path=... --tunnel=ssh --ssh-jump=bastion --ssh-user=jumper --ssh-password=...`

2. **Translate into MCP tool params:** `command`, `host`, `port` (default 22),
   `user`, `password` OR `key_path` (mutually exclusive on the target).
   Plus the global tunnel/ssh_* fields when `--tunnel=ssh` is used —
   those are the JUMP credentials, separate from target creds.

3. **Call `ssh_exec`** with the params from Step 2.

4. **Render the result.** The response is an ExecutionResult with rows
   `["exit_code", "..."]`, `["stdout", "..."]`, `["stderr", "..."]`.
   - Show stdout to the user as text (markdown code block if it looks
     like structured output).
   - If `exit_code` is non-zero, mention it explicitly and show stderr.
   - If exit_code is 0 and stderr is non-empty, show stderr as a warning.

5. **Destructive commands** (anything that modifies state on the remote:
   `rm`, `mv`, `kill`, `systemctl restart`, `apt install`, `dd`, etc.):
   pause and confirm with the user BEFORE calling the tool. Especially
   if the command starts with `sudo` or runs as root.

## When something fails

- `Error::Config("SSH target requires --password or --key-path")` →
  the user supplied neither auth method.
- `Error::Connection("SSH connect to ... failed")` → can't reach the
  target on the SSH port. Check host/port; if going through a jump,
  the jump may be the problem (see ssh-bastion-checklist).
- `Error::Connection("SSH publickey/password auth failed")` → wrong
  creds. Note: TARGET creds are checked separately from JUMP creds.
- `Error::Service("SSH ...")` → russh-level error (channel open,
  exec request, etc.). Usually means the SSH session was terminated
  unexpectedly or the remote refused the channel.
- Commands needing a TTY (e.g. `top`, `htop`, `vim`) → fail because
  this Phase doesn't allocate a PTY. Run non-interactive variants
  (e.g. `top -bn1`) instead.
- SSH jump errors → use the **ssh-bastion-checklist** skill.
```

- [ ] **Step 2: `ssh-using` skill**

Create `skills/ssh-using/SKILL.md`:

```markdown
---
name: ssh-using
description: Use when calling the `ssh_exec` MCP tool from the tools-mcp plugin — explains target creds vs jump creds, output mapping (exit_code / stdout / stderr), PTY limitations, and common error shapes.
---

# Using the `ssh_exec` MCP tool

`tools-mcp` exposes an `ssh_exec` MCP tool. Runs one shell command on a target SSH server, returns exit_code + stdout + stderr in a flat ExecutionResult. Phase 7: no profile/YAML — just CLI/MCP fields.

## Tool input

```json
{
  "command":  "ls -la /var/log",         // required
  "host":     "server.com",              // required
  "port":     22,                        // optional, default 22
  "user":     "admin",                   // required
  "password": "...",                     // OR
  "key_path": "/home/me/.ssh/id_rsa",    // (mutually exclusive)

  // optional tunnel — these are the JUMP credentials, NOT target's
  "tunnel":   "ssh",
  "ssh_jump": "bastion.com",
  "ssh_user": "jumper",
  "ssh_password": "...",
  "ssh_key_path": "/home/me/.ssh/jump_key",
  "ssh_port": 22
}
```

## Two credential sets

This is the most important thing to understand:

- **Target creds** (`user`, `password` / `key_path`, `port`) — for the SSH server where the command runs.
- **Jump creds** (`ssh_user`, `ssh_password` / `ssh_key_path`, `ssh_port`) — for the bastion(s) you go through to reach the target. ALL jumps share the same set.

If target and jump use the same credentials, you still need to supply both (the tool doesn't infer one from the other).

If `tunnel` is omitted or set to `"direct"`, no jumps are used and the target is reached directly.

## Output shape

ExecutionResult:

| field | value |
| --- | --- |
| `exit_code` | `0` |
| `stdout` | `total 12\\ndrwxr-xr-x ...` |
| `stderr` | `` |

`exit_code = 0` means the command succeeded. Non-zero means it failed; show stderr to help the user understand why.

`<unknown>` for exit_code means the SSH channel closed without the remote sending an exit status — usually the connection died mid-execution. Treat as a failure.

Bytes are UTF-8-decoded if possible; binary stdout (rare for shell commands) renders as `<N bytes (non-UTF-8)>`.

## Common workflows

- **Check a service is up**: `systemctl status nginx` (exit_code 0 = active).
- **Tail a log**: `tail -n 100 /var/log/syslog` (then optionally pipe to local rg).
- **List process count**: `ps aux | wc -l`.
- **Disk free**: `df -h`.
- **Memory**: `free -h`.
- **Time on remote**: `date -u`.

For LARGER tasks (multi-minute), `ssh_exec` is fine — it waits for the channel to close. There's no streaming; the tool returns when the remote closes stdout.

## PTY / TTY limitation

`ssh_exec` does NOT allocate a PTY. Commands that require a TTY (e.g. `top`, `htop`, `vim`, `passwd`, anything calling `isatty(stdin)`) will fail or behave unexpectedly. Use non-interactive variants (`top -bn1`, etc.) or run the command via `bash -c '...'` if shell features are needed.

## Destructive commands

Confirm with the user BEFORE running:
- `rm`, `unlink`, `find ... -delete`
- `mv` to overwrite paths
- `dd` writing to disk
- `mkfs.*`
- `systemctl restart` / `reboot` / `shutdown`
- `apt install` / `apt remove` / `pip install` / etc.
- `kill -9`, `pkill`
- Anything starting with `sudo` (escalation)

Read-only commands (`ls`, `cat`, `grep`, `df`, `free`, `ps`, `systemctl status`, `journalctl`, `dmesg`, `find ... -type f`) are safe to run without a confirmation prompt.

## Common error shapes

- `Error::Config("SSH target requires --password or --key-path")` — neither auth method supplied.
- `Error::Config("password and key_path are mutually exclusive")` — both supplied for the target.
- `Error::Connection("SSH connect to ... failed")` — can't reach the target SSH port.
- `Error::Connection("SSH publickey/password auth failed")` — wrong target credentials. (Note: jump auth uses different fields, so a wrong password here doesn't affect the jump chain.)
- `Error::Connection("SSH password authentication rejected")` — the password fallback (after publickey) was rejected by the server.
- `Error::Service("SSH session open failed")` — russh couldn't open a session channel on the target. Usually means the server killed the connection.
- `Error::Service("SSH exec request failed")` — server refused the exec request (rare; possibly forced-command setup or a restricted shell).
- SSH jump errors → use the **ssh-bastion-checklist** skill (the failures there apply equally to the jumps used by ssh_exec).

## What this skill is NOT

- Not for SCP/SFTP file transfer (Phase 8+).
- Not for interactive shells / PTY-required commands.
- Not for `mysql_exec` / `redis_exec` / `http_exec` — see the respective skills.
```

- [ ] **Step 3: Verify**

Run: `ls commands/ skills/`
Expected: `commands/{http,mysql,redis,ssh}.md`, `skills/{tools-mcp-using,mysql-debugging,redis-using,http-using,ssh-using,ssh-bastion-checklist}/SKILL.md`.

- [ ] **Step 4: Commit**

```bash
git add commands/ssh.md skills/ssh-using/
git commit -m "feat(plugin): add /ssh slash command + ssh-using skill

- /ssh \"<COMMAND>\" --host=... --user=... ... — calls ssh_exec.
- ssh-using skill — distinguishes target creds vs jump creds; documents
  output shape (exit_code/stdout/stderr); PTY limitation; destructive-
  command list; common error shapes.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Documentation + final verification

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `AGENTS.md`

- [ ] **Step 1: README — Status section**

Replace the existing `## Status` section. Update the implemented list and remove ssh-direct from "not yet implemented":

```markdown
## Status

This is the Phase 7 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`) and `mysql_exec` MCP tool.
- Redis CLI mode (`tools-mcp redis "..."`) and `redis_exec` MCP tool.
- HTTP CLI mode (`tools-mcp http GET https://...`) and `http_exec` MCP tool.
- **SSH-direct CLI mode** (`tools-mcp ssh "..."`) and `ssh_exec` MCP tool —
  run a shell command on a target SSH server, optionally through SSH jump hosts.
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
  for MySQL and Redis. (HTTP and SSH-direct profile/YAML is Phase 8+.)
- Direct connection (`--tunnel=direct` or no `--tunnel`).
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth. Host keys accepted with a fingerprint warning.
  Works for HTTP and SSH-direct too.
- MCP server mode (`tools-mcp` with no subcommand) over stdio.

Not yet implemented:
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
- SSH PTY allocation (interactive commands like `top` won't work)
- HTTP / SSH-direct profile/YAML config
- HTTP/SSE MCP transport (the SERVER's transport)
- Redis cluster routing, pub/sub, transactions, scripting (EVAL)
- Per-Value typed mapping for RESP3 `Map` / `Set` / `Push`
- SCP/SFTP file transfer
```

- [ ] **Step 2: README — Usage subsection for SSH**

After the existing `### HTTP` subsection (and before `### MCP Server`), insert:

````markdown
### SSH (remote command execution)

```bash
# Direct connection
tools-mcp ssh "uname -a" --host=server.com --user=admin --key-path=~/.ssh/id_rsa

# With password
tools-mcp ssh "df -h" --host=10.0.0.5 --user=root --password=secret

# Through an SSH jump (jump creds are SEPARATE from target creds)
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=jumper --ssh-password=jpwd \
  ssh "systemctl status nginx" --host=internal-server --user=admin --key-path=~/.ssh/target_key

# Show structured output (exit_code/stdout/stderr table)
tools-mcp ssh "false" --host=h --user=u --key-path=~/.ssh/k -i
```

By default tools-mcp's exit code mirrors the remote command's exit code, so
shell-script usage works (e.g. `if tools-mcp ssh "test -f /etc/passwd" ...`).
````

- [ ] **Step 3: README — Plugin assets list**

Update the "What the plugin provides" block:

```markdown
What the plugin provides:

- **MCP tools** auto-registered via `.mcp.json`:
  - `mysql_exec` — run a MySQL query.
  - `redis_exec` — run a Redis command.
  - `http_exec` — send an HTTP request.
  - `ssh_exec` — run a shell command on a remote SSH server.
- **Skills** that guide the assistant:
  - `tools-mcp-using` — parameter shape, three-layer config priority, multi-hop syntax (mysql + redis).
  - `mysql-debugging` — diagnostic queries for common MySQL errors, locks, slow queries.
  - `redis-using` — Redis command shape, output mapping, destructive-command list.
  - `http-using` — HTTP tool input, tunnel routing for internal HTTPS, output mapping.
  - `ssh-using` — SSH-direct target/jump cred separation, output mapping, PTY limits.
  - `ssh-bastion-checklist` — narrows down SSH-tunnel failures.
- **Slash commands**:
  - `/mysql <SQL>` — quick MySQL query.
  - `/redis <COMMAND>` — quick Redis command.
  - `/http <METHOD> <URL>` — quick HTTP request.
  - `/ssh <COMMAND>` — quick remote shell command.
```

- [ ] **Step 4: CLAUDE.md and AGENTS.md updates**

Apply identical edits to both files.

a) **Project Overview lead sentence** — update to Phase 7:

Before:
```markdown
`tools-mcp` is a Rust CLI + MCP server for HTTP, MySQL, Redis, and SSH. **Phase 6 (current) implements MySQL + Redis + HTTP CLI modes and matching MCP tools (`mysql_exec`, `redis_exec`, `http_exec`)**; SSH direct is the remaining service phase boundary.
```

After:
```markdown
`tools-mcp` is a Rust CLI + MCP server for HTTP, MySQL, Redis, and SSH. **Phase 7 (current) implements MySQL + Redis + HTTP + SSH-direct CLI modes and matching MCP tools (`mysql_exec`, `redis_exec`, `http_exec`, `ssh_exec`)**. All four supported services are now shipped.
```

b) **Module map** — add a new row for `tools-mcp-ssh` after `tools-mcp-redis`, and a row for `core::ssh` after `core::http`:

```markdown
| `tools-mcp-ssh` (lib) | `AcceptAnyHostKey` / `authenticate` / `build_session_chain` (the Phase 2 helpers, now shared between the bin's `SshTunnel` and this lib's `SshExec`); `SshExecRequest` (host/port/user/password/key_path/command); `execute(req, jumps)` entry. Owns the russh `session` channel + `exec` request glue. Maps responses to flat `field`/`value` rows (exit_code / stdout / stderr). |
```

```markdown
| `tools-mcp` bin (root `src/core/ssh.rs`) | Orchestrator `execute(SshExecRequest, Option<TunnelConfig>)`: validate target has password OR key_path, translate `TunnelConfig::Ssh` to `SshJumpsConfig`, dispatch into the ssh lib. Doesn't take a `Config` (Phase 7 deferred Profile/YAML for ssh-direct, same as Phase 6 HTTP). |
```

c) **Phase boundaries** — add an entry for SSH-direct after HTTP. Replace the existing `SSH-direct subcommand: not yet implemented` entry with:

```markdown
- **SSH-direct subcommand**: implemented in Phase 7. `tools-mcp ssh "<COMMAND>"` and the `ssh_exec` MCP tool both route through `core::ssh::execute`. TARGET credentials are separate from JUMP credentials (when `--tunnel=ssh` is used) — that's by design (Model A). Reuses the Phase 2 multi-hop infrastructure via `tools_mcp_ssh::session::build_session_chain` (extracted from the bin in Phase 7 Task 2 so both `SshTunnel` and `SshExec` share it). Phase 7 deliberately doesn't support Profile/YAML — only CLI flags + global tunnel.
```

d) **Conventions worth knowing** — append:

```markdown
- **Two-credential separation for ssh-direct**: when SSH is BOTH the tunnel transport AND the target service (i.e. `tools-mcp ssh ... --tunnel=ssh`), the JUMP credentials (`ssh_user`/`ssh_password`/`ssh_key_path`) and the TARGET credentials (`user`/`password`/`key_path`) are independent. Don't infer one from the other. The session chain authenticates with jump creds; the final SSH session (over the last jump's direct-tcpip channel) authenticates with target creds.
```

- [ ] **Step 5: Verify CLAUDE.md and AGENTS.md only differ on the cross-link**

Run: `diff <(tail -n +5 CLAUDE.md) <(tail -n +5 AGENTS.md)`
Expected: only the cross-link blockquote line + pre-existing methodology trailer line difference.

- [ ] **Step 6: Final workspace verification**

Run: `cargo test`
Expected: all tests pass. Report the actual count.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean. If diff shows up, run `cargo fmt --all` and re-verify.

Run: `cargo build --release`
Expected: workspace builds; binary at `target/release/tools-mcp`.

Run: `./target/release/tools-mcp ssh --help | head -3`
Expected:
```
Execute a shell command on an SSH target

Usage: tools-mcp [GLOBAL OPTIONS] ssh [OPTIONS] <COMMAND>
```

Regression checks for the existing 3 services:
```bash
for svc in mysql redis http; do ./target/release/tools-mcp $svc --help | head -3; done
```
All three should still print their respective headers.

**Optional manual smoke (only if a real SSH target is available):**
```bash
./target/release/tools-mcp ssh "whoami" --host=<your-server> --user=<user> --key-path=<key>
```
Should print the remote username.

- [ ] **Step 7: Commit**

```bash
git add README.md CLAUDE.md AGENTS.md
git commit -m "docs: document Phase 7 SSH-direct support

- README Status: Phase 7; SSH-direct usage examples; updated plugin
  asset list (4 tools, 6 skills, 4 slash commands).
- CLAUDE.md / AGENTS.md: lead sentence updated; module map adds
  tools-mcp-ssh lib + core::ssh orchestrator rows; Phase boundaries
  record SSH-direct as shipped (no service boundaries remain);
  conventions add a 'two-credential separation for ssh-direct' note.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Summary

After Phase 7:

- `tools-mcp ssh "<COMMAND>"` works as a CLI subcommand with `--host`/`--port`/`--user`/`--password`/`--key-path`.
- The `ssh_exec` MCP tool exposes the same surface to AI clients.
- Both share the orchestrator at `core::ssh::execute`.
- The new `tools-mcp-ssh` lib crate owns the russh `session` + `exec` glue AND houses the shared `build_session_chain` (extracted from the bin's `SshTunnel` so both can use it without duplication).
- The plugin ships a `/ssh` slash command + an `ssh-using` skill.
- TARGET creds and JUMP creds are independent — Model A from the original design spec.
- Architecture invariant holds: every CLI subcommand has a paired MCP tool. **All 4 supported services (MySQL, Redis, HTTP, SSH-direct) are now shipped.**

**Deferred to Phase 8+:**
- HTTP / SSH-direct profile/YAML config.
- Redis cluster routing, pub/sub, transactions.
- Per-Value typed mapping for RESP3 `Map` / `Set` / `Push`.
- SCP/SFTP file transfer (would be a `tools-mcp-sftp` lib).
- SSH PTY allocation for interactive commands.
- SSH key passphrases, per-hop auth overrides, strict known_hosts.
- HTTP cookie jar, redirects beyond reqwest default, streaming downloads, WebSocket / SSE.
- Cleanup pass: extract a shared `build_tunnel_config_for_<svc>` helper across all 4 MCP tools.
