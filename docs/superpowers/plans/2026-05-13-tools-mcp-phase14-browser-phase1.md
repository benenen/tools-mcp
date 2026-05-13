# Tools MCP Phase 14: Browser Support (Phase 1 — shell-out to agent-browser)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a seventh tool to tools4a — `tools4a browser <subcommand> [args...]` CLI + `browser_exec` MCP tool — that shells out to the externally-installed [`vercel-labs/agent-browser`](https://github.com/vercel-labs/agent-browser) CLI. tools4a does not embed a browser, does not own session state, and does not implement the Chrome DevTools Protocol. It is a thin, structured wrapper that gives MCP clients a typed entry point into agent-browser's stateful daemon model.

**Architecture:** New `tools4a-browser` leaf crate that mirrors the existing vertical-slice pattern (`request.rs` / `exec.rs` / `execute.rs` / `orchestrator.rs` / `mcp.rs` / `lib.rs`) — same shape as `tools4a-ssh` and `tools4a-http`. The leaf crate uses `tokio::process::Command` to spawn the external `agent-browser` binary, captures stdout / stderr / exit code, and maps them into the standard `ExecutionResult`. The browser daemon owns all session / page / cookie state — each MCP call is one short-lived CLI invocation against the persistent daemon (same shape as the existing "one exec → ExecutionResult" model the other six tools already use).

**Why Phase 1 ships without SSH tunnel:** the existing `--tunnel=ssh` implementation forwards a single `direct-tcpip` channel to one host:port. A browser is a full network stack and needs **SOCKS-shaped** routing (or per-connection CONNECT) to reach arbitrary remote hosts cleanly — cookies, Host header, TLS SNI, sub-resource requests would all break under naive single-port forwarding. Building a SOCKS5 server on top of russh `channel_open_direct_tcpip` is the right answer but a separate piece of infrastructure (`tools4a-core::tunnel::SocksTunnel`). Phase 1 ships the wiring; **Phase 2 (deferred)** adds SocksTunnel and wires it through. Until then, `--tunnel=ssh` for browser is a config error with an explicit "deferred to Phase 2" message; users who already have their own `ssh -D 1080` can use `--proxy socks5://127.0.0.1:1080` (passthrough to agent-browser).

**Pre-requisite:** the operator must have installed `agent-browser` separately (e.g. `npm i -g agent-browser` or the Rust binary build per upstream). tools4a never downloads it. Binary lookup order: `$AGENT_BROWSER_BIN` env var → `agent-browser` on `$PATH` → `Error::Config` with install hint.

**Out of scope (deferred):**
- SSH tunnel for browser (Phase 2 — needs `SocksTunnel` in `tools4a-core`).
- Profile/YAML config for browser default `--proxy` / `--args` / `--session`. Phase 1 = CLI flags + MCP fields only (same minimalism as the original Phase 6 HTTP shipped with).
- Bundling / managing `agent-browser` (install / upgrade / version pinning).
- Owning session state inside tools4a (the daemon does it).
- A dedicated MCP App UI resource (the SQL / HTTP tools have one; browser output is heterogeneous — defer until usage patterns are clearer).

---

## File Structure

**New:**
- `crates/tools4a-browser/Cargo.toml` — `tokio` (process feature) + `tools4a-core` + `async-trait` + `schemars` + `serde`.
- `crates/tools4a-browser/src/lib.rs` — module declarations + re-exports.
- `crates/tools4a-browser/src/request.rs` — `BrowserRequest` (subcommand + args + session + proxy + bin path).
- `crates/tools4a-browser/src/exec.rs` — `BrowserExec` runner + `BrowserOutput` capture (stdout / stderr / exit_code) + `output_to_result`.
- `crates/tools4a-browser/src/execute.rs` — `execute(req)` entry (no tunnel arg — Phase 1 only supports direct).
- `crates/tools4a-browser/src/orchestrator.rs` — `BrowserOrchestrator` (`impl Service`) — validates that `tunnel` is `None` / `Direct` and rejects `Ssh` with a Phase 2 message.
- `crates/tools4a-browser/src/mcp.rs` — `BrowserExecParams` + `BrowserMcp` (`impl McpTool`).
- `commands/browser.md` — slash command.
- `skills/browser-using/SKILL.md` — usage skill.

**Modified:**
- `Cargo.toml` (workspace) — add `crates/tools4a-browser` to `members`; add bin `[dependencies]` line.
- `src/cli/args.rs` — add `Commands::Browser { subcommand, args, session, proxy, proxy_bypass, browser_args, bin }`.
- `src/cli/handler.rs` — handle the new variant; add `execute_browser`.
- `src/mcp/server.rs` — register `#[tool] browser_exec` (one-liner that calls `into_call_result(BrowserMcp::invoke(params).await)`).
- `tests/mcp_smoke.rs` — assert `browser_exec` appears in `tools/list`.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — document Phase 14.

---

## Task 1: Bootstrap empty `tools4a-browser` leaf crate

**Files:**
- Create: `crates/tools4a-browser/Cargo.toml`
- Create: `crates/tools4a-browser/src/lib.rs`
- Modify: root `Cargo.toml`

- [ ] **Step 1: `crates/tools4a-browser/Cargo.toml`**

```toml
[package]
name = "tools4a-browser"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
schemars = "1.0"
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.40", features = ["process", "io-util", "macros"] }
tools4a-core = { path = "../tools4a-core" }

[dev-dependencies]
tokio = { version = "1.40", features = ["macros", "rt-multi-thread"] }
```

The `process` feature on tokio is the only addition relative to the leaner ssh / http crates — we need `tokio::process::Command`.

- [ ] **Step 2: `crates/tools4a-browser/src/lib.rs`**

```rust
//! Browser stack: thin wrapper around the external `agent-browser`
//! CLI (https://github.com/vercel-labs/agent-browser). tools4a does
//! not embed a browser; the daemon spawned by agent-browser owns all
//! session / page / cookie state. Each call here is one short-lived
//! CLI invocation against that persistent daemon, captured as an
//! `ExecutionResult` (stdout / stderr / exit_code).
```

Leave it as a doc-comment-only stub for this task. Subsequent tasks add `pub mod` lines.

- [ ] **Step 3: Workspace + bin dep**

In root `Cargo.toml`:

a) Add `"crates/tools4a-browser",` to `[workspace] members` (alphabetical).

b) Add to bin `[dependencies]` (alphabetical):

```toml
tools4a-browser = { path = "crates/tools4a-browser" }
```

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
```

Expected: clean build; existing test count passes (no new tests yet).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(browser): scaffold empty tools4a-browser leaf crate

New workspace member crates/tools4a-browser with the standard leaf
shape (deps on tools4a-core + async-trait + schemars + serde +
tokio with the process feature for spawning external commands).
lib.rs is a doc-comment-only stub at this point — subsequent tasks
add request / exec / execute / orchestrator / mcp modules.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `BrowserRequest` input type

**Files:**
- Create: `crates/tools4a-browser/src/request.rs`
- Modify: `crates/tools4a-browser/src/lib.rs`

- [ ] **Step 1: Define the request type**

Create `crates/tools4a-browser/src/request.rs`:

```rust
//! Browser request input shape — independent of any caller (CLI, MCP).

/// One agent-browser CLI invocation.
///
/// Modeled after `SshExecRequest`: the caller fully specifies the
/// command line; tools4a doesn't parse or rewrite agent-browser
/// subcommands. agent-browser owns the subcommand surface — adding a
/// new one upstream needs no change here.
#[derive(Debug, Clone)]
pub struct BrowserRequest {
    /// Subcommand to invoke (e.g. `open`, `click`, `snapshot`, `batch`,
    /// `eval`, `cookies`, `screenshot`). Passed through verbatim as the
    /// first positional argument to `agent-browser`.
    pub subcommand: String,

    /// Positional + flag arguments that follow `<subcommand>`. Passed
    /// directly to `Command::args` — no shell interpretation.
    pub args: Vec<String>,

    /// Optional `--session <NAME>` to isolate daemon state. None = use
    /// agent-browser's default session.
    pub session: Option<String>,

    /// Optional `--proxy <URL>` (e.g. `socks5://127.0.0.1:1080` for
    /// users who set up their own SSH SOCKS forward via `ssh -D`).
    pub proxy: Option<String>,

    /// Optional `--proxy-bypass <hosts>` (comma-separated).
    pub proxy_bypass: Option<String>,

    /// Optional `--args <flags>` — extra Chromium launch arguments.
    pub browser_args: Option<String>,

    /// Path to the agent-browser binary. If None, the runner looks up
    /// `$AGENT_BROWSER_BIN`, then falls back to `agent-browser` on `$PATH`.
    pub bin: Option<std::path::PathBuf>,
}
```

- [ ] **Step 2: Wire `request` into `lib.rs`**

```rust
//! Browser stack: thin wrapper around the external `agent-browser`
//! CLI ...

pub mod request;

pub use request::BrowserRequest;
```

- [ ] **Step 3: Verify**

```bash
cargo build
cargo test
```

Expected: clean; same test count.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(browser): BrowserRequest input type

Plain data type describing one agent-browser CLI invocation:
subcommand + args + optional session / proxy / proxy_bypass /
browser_args / bin override. Caller (CLI handler / MCP tool) fills
this; the lib doesn't parse subcommands.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `BrowserExec` runner + `output_to_result`

**Files:**
- Create: `crates/tools4a-browser/src/exec.rs`
- Modify: `crates/tools4a-browser/src/lib.rs`

- [ ] **Step 1: Write the runner**

Create `crates/tools4a-browser/src/exec.rs`:

```rust
//! Spawn the external `agent-browser` binary and capture
//! stdout / stderr / exit code into the standard `ExecutionResult`
//! shape. Modeled after `tools4a_ssh::exec` — same field layout so
//! MCP clients get a consistent shape across `ssh_exec` and
//! `browser_exec`.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;
use tools4a_core::{Error, ExecutionResult, Result};

use crate::request::BrowserRequest;

/// Captured output of one agent-browser invocation.
#[derive(Debug, Clone)]
pub struct BrowserOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Map a captured BrowserOutput to the standard 3-row ExecutionResult.
/// Layout matches `tools4a_ssh::output_to_result`.
pub fn output_to_result(out: BrowserOutput) -> ExecutionResult {
    let rows = vec![
        vec!["exit_code".to_string(), out.exit_code.to_string()],
        vec!["stdout".to_string(), out.stdout],
        vec!["stderr".to_string(), out.stderr],
    ];
    let affected = rows.len() as u64;
    ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        rows,
        affected,
    )
}

/// Resolve which binary to invoke.
///
/// Priority: explicit `req.bin` -> `$AGENT_BROWSER_BIN` env ->
/// `"agent-browser"` (let `Command` walk `$PATH`).
pub fn resolve_bin(req: &BrowserRequest) -> PathBuf {
    if let Some(p) = &req.bin {
        return p.clone();
    }
    if let Ok(s) = std::env::var("AGENT_BROWSER_BIN") {
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    PathBuf::from("agent-browser")
}

pub struct BrowserExec;

impl BrowserExec {
    pub async fn run(req: BrowserRequest) -> Result<BrowserOutput> {
        let bin = resolve_bin(&req);

        let mut cmd = Command::new(&bin);
        cmd.arg(&req.subcommand);
        for a in &req.args {
            cmd.arg(a);
        }
        if let Some(s) = &req.session {
            cmd.arg("--session").arg(s);
        }
        if let Some(p) = &req.proxy {
            cmd.arg("--proxy").arg(p);
        }
        if let Some(b) = &req.proxy_bypass {
            cmd.arg("--proxy-bypass").arg(b);
        }
        if let Some(a) = &req.browser_args {
            cmd.arg("--args").arg(a);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Config(format!(
                    "agent-browser binary not found at '{}'. Install with `npm i -g agent-browser` \
                     or the upstream Rust build, then ensure it's on $PATH (or set $AGENT_BROWSER_BIN). \
                     Upstream: https://github.com/vercel-labs/agent-browser",
                    bin.display()
                ))
            } else {
                Error::Service(format!("agent-browser spawn failed: {e}"))
            }
        })?;

        Ok(BrowserOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(sub: &str) -> BrowserRequest {
        BrowserRequest {
            subcommand: sub.into(),
            args: Vec::new(),
            session: None,
            proxy: None,
            proxy_bypass: None,
            browser_args: None,
            bin: None,
        }
    }

    #[test]
    fn resolve_bin_explicit_wins() {
        let r = BrowserRequest {
            bin: Some(PathBuf::from("/opt/ab")),
            ..req("open")
        };
        assert_eq!(resolve_bin(&r), PathBuf::from("/opt/ab"));
    }

    #[test]
    fn resolve_bin_env_used_when_no_explicit() {
        unsafe {
            std::env::set_var("AGENT_BROWSER_BIN", "/etc/ab-from-env");
        }
        let got = resolve_bin(&req("open"));
        unsafe {
            std::env::remove_var("AGENT_BROWSER_BIN");
        }
        assert_eq!(got, PathBuf::from("/etc/ab-from-env"));
    }

    #[test]
    fn resolve_bin_falls_back_to_path_name() {
        unsafe {
            std::env::remove_var("AGENT_BROWSER_BIN");
        }
        assert_eq!(resolve_bin(&req("open")), PathBuf::from("agent-browser"));
    }

    #[test]
    fn output_to_result_layout() {
        let r = output_to_result(BrowserOutput {
            stdout: "hi\n".into(),
            stderr: "warn\n".into(),
            exit_code: 0,
        });
        assert_eq!(r.columns, vec!["field".to_string(), "value".to_string()]);
        assert_eq!(r.rows.len(), 3);
        assert_eq!(r.rows[0], vec!["exit_code".to_string(), "0".to_string()]);
        assert_eq!(r.rows[1], vec!["stdout".to_string(), "hi\n".to_string()]);
        assert_eq!(
            r.rows[2],
            vec!["stderr".to_string(), "warn\n".to_string()]
        );
    }

    #[tokio::test]
    async fn run_reports_missing_binary_clearly() {
        let r = BrowserRequest {
            bin: Some(PathBuf::from("/nonexistent/agent-browser-xyz")),
            ..req("open")
        };
        let err = BrowserExec::run(r).await.unwrap_err();
        match err {
            Error::Config(msg) => {
                assert!(msg.contains("not found"), "got: {msg}");
                assert!(msg.contains("agent-browser"), "got: {msg}");
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }
}
```

> **Note on env-var tests:** they manipulate process-global state. The
> three `resolve_bin_*` tests are interdependent if `cargo test` runs them
> in parallel. Acceptable for Phase 1 because no other test reads
> `AGENT_BROWSER_BIN`; if a future test does, gate them with `#[ignore]`
> + a `--test-threads=1` script.

- [ ] **Step 2: Wire `exec` into `lib.rs`**

```rust
pub mod exec;
pub mod request;

pub use exec::{BrowserExec, BrowserOutput, output_to_result, resolve_bin};
pub use request::BrowserRequest;
```

- [ ] **Step 3: Verify**

```bash
cargo test --package tools4a-browser
```

Expected: 5 PASS (3 resolve_bin + output_to_result_layout + run_reports_missing_binary_clearly).

```bash
cargo test
```

Expected: prior + 5 new = pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(browser): BrowserExec runner + output_to_result

Runs the external agent-browser binary with the caller's subcommand
+ args, plus optional --session / --proxy / --proxy-bypass / --args
passthroughs. Captures stdout / stderr / exit_code. resolve_bin
walks: req.bin -> \$AGENT_BROWSER_BIN -> 'agent-browser' on \$PATH.
ENOENT becomes a clear Error::Config with install hint instead of
an opaque os error.

output_to_result mirrors tools4a_ssh's 3-row layout (exit_code /
stdout / stderr) so MCP clients see a consistent shape across
ssh_exec and browser_exec.

5 unit tests cover binary resolution priority, output mapping, and
the missing-binary error message.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `execute(req)` entry function

**Files:**
- Create: `crates/tools4a-browser/src/execute.rs`
- Modify: `crates/tools4a-browser/src/lib.rs`

- [ ] **Step 1: Write the entry**

Create `crates/tools4a-browser/src/execute.rs`:

```rust
//! Top-level entry: run one agent-browser invocation and return the
//! structured result. No tunnel handling here — Phase 1 only supports
//! direct execution; the orchestrator validates `TunnelConfig::Ssh`
//! is not set and surfaces a Phase 2 deferral message.

use tools4a_core::{ExecutionResult, Result};

use crate::exec::{BrowserExec, output_to_result};
use crate::request::BrowserRequest;

pub async fn execute(req: BrowserRequest) -> Result<ExecutionResult> {
    let out = BrowserExec::run(req).await?;
    Ok(output_to_result(out))
}
```

- [ ] **Step 2: Wire `execute` into `lib.rs`**

```rust
pub mod exec;
pub mod execute;
pub mod request;

pub use exec::{BrowserExec, BrowserOutput, output_to_result, resolve_bin};
pub use execute::execute;
pub use request::BrowserRequest;
```

- [ ] **Step 3: Verify**

```bash
cargo build
cargo test
```

Expected: same count as Task 3 (this is just a thin composition; integration is exercised through the orchestrator's tests in Task 5).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(browser): execute(req) entry composition

Thin wrapper: BrowserExec::run -> output_to_result. Phase 1 doesn't
take a tunnel argument because the leaf orchestrator rejects ssh
upstream. The signature stays argument-light so Phase 2 can add an
optional SocksTunnel without a breaking change to internal callers.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: `BrowserOrchestrator: impl Service`

**Files:**
- Create: `crates/tools4a-browser/src/orchestrator.rs`
- Modify: `crates/tools4a-browser/src/lib.rs`

- [ ] **Step 1: Write the orchestrator**

Create `crates/tools4a-browser/src/orchestrator.rs`:

```rust
//! BrowserOrchestrator — `Service` impl for the browser tool.
//!
//! Validates the tunnel kind (Phase 1 only allows None / Direct;
//! TunnelConfig::Ssh is rejected with an explicit Phase 2 deferral
//! message), then dispatches into `execute`.

use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig};

use crate::execute::execute;
use crate::request::BrowserRequest;

pub struct BrowserOrchestrator;

#[async_trait]
impl Service for BrowserOrchestrator {
    type Request = BrowserRequest;

    async fn execute(
        req: Self::Request,
        tunnel: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        match tunnel {
            None | Some(TunnelConfig::Direct) => execute(req).await,
            Some(TunnelConfig::Ssh { .. }) => Err(Error::Config(
                "tunnel=ssh is not supported for the browser tool in Phase 1. \
                 The current SSH tunnel forwards a single TCP port (direct-tcpip), \
                 which doesn't fit a full browser's network stack (cookies / SNI / \
                 Host header / sub-resources). Phase 2 will add SOCKS5 routing \
                 through SSH. As a workaround, run `ssh -D 1080 <bastion>` yourself \
                 and pass `--proxy socks5://127.0.0.1:1080` (CLI) or `\"proxy\": \
                 \"socks5://127.0.0.1:1080\"` (MCP)."
                    .to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> BrowserRequest {
        BrowserRequest {
            subcommand: "snapshot".into(),
            args: Vec::new(),
            session: None,
            proxy: None,
            proxy_bypass: None,
            browser_args: None,
            bin: Some(std::path::PathBuf::from("/nonexistent/ab")),
        }
    }

    #[tokio::test]
    async fn rejects_ssh_tunnel_with_phase2_message() {
        let err = BrowserOrchestrator::execute(
            req(),
            Some(TunnelConfig::Ssh {
                ssh_jumps: vec![("bastion.example.com".to_string(), 22)],
                ssh_user: "admin".to_string(),
                ssh_password: None,
                ssh_key_path: None,
                ssh_port: 22,
            }),
        )
        .await
        .unwrap_err();
        match err {
            Error::Config(m) => {
                assert!(m.contains("Phase 2"), "got: {m}");
                assert!(m.contains("socks5://"), "got: {m}");
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn accepts_none_tunnel() {
        let err = BrowserOrchestrator::execute(req(), None).await.unwrap_err();
        match err {
            Error::Config(m) => assert!(m.contains("not found"), "got: {m}"),
            other => panic!("expected Config, got {other:?}"),
        }
    }
}
```

Re-check `tools4a-core`'s `TunnelConfig::Ssh` variant fields before
committing — the struct literal in the test must match. Cross-reference
with `crates/tools4a-mysql/src/orchestrator.rs` or `tools4a-ssh`'s test
fixtures for the canonical shape; if the fields differ, copy from there.

- [ ] **Step 2: Wire `orchestrator` into `lib.rs` (without `mcp` yet)**

```rust
pub mod exec;
pub mod execute;
pub mod orchestrator;
pub mod request;

pub use exec::{BrowserExec, BrowserOutput, output_to_result, resolve_bin};
pub use execute::execute;
pub use orchestrator::BrowserOrchestrator;
pub use request::BrowserRequest;
```

- [ ] **Step 3: Verify**

```bash
cargo test --package tools4a-browser
```

Expected: 5 prior + 2 new (orchestrator tests) = 7 PASS.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: workspace tests pass; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(browser): BrowserOrchestrator impl Service (Phase 1)

Validates the tunnel kind: None/Direct dispatch into execute; Ssh
returns Error::Config with an explicit Phase 2 deferral message
pointing users at 'ssh -D 1080' + --proxy as the manual workaround.

The reasoning lives in the error message rather than a comment — the
LLM client surfacing the error gets the workaround documented
inline, no skill lookup needed.

2 unit tests: ssh tunnel rejection includes the Phase 2 keyword and
the socks5 workaround; None tunnel passes through to execute (and
surfaces the missing-binary error, confirming the guard didn't
short-circuit).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: `BrowserExecParams` + `BrowserMcp: impl McpTool`

**Files:**
- Create: `crates/tools4a-browser/src/mcp.rs`
- Modify: `crates/tools4a-browser/src/lib.rs`

- [ ] **Step 1: Write the MCP params + tool**

Create `crates/tools4a-browser/src/mcp.rs`:

```rust
//! `browser_exec` MCP tool — params + `McpTool` impl. Same shape as
//! `tools4a_ssh::mcp`: params land directly in a typed Request +
//! TunnelConfig, then dispatch through the orchestrator.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tools4a_core::{
    ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

use crate::orchestrator::BrowserOrchestrator;
use crate::request::BrowserRequest;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BrowserExecParams {
    /// agent-browser subcommand (e.g. "open", "click", "snapshot",
    /// "batch", "eval", "cookies", "screenshot"). Passed through
    /// verbatim; tools4a does not enumerate or validate the set —
    /// adding new subcommands upstream needs no change here.
    pub subcommand: String,

    /// Positional + flag arguments after the subcommand. No shell
    /// interpretation; each entry becomes one argv element.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// agent-browser `--session <NAME>` — isolates daemon state. Use
    /// the same value across calls to share cookies / pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,

    /// agent-browser `--proxy <URL>`. Phase 1: pass this yourself if
    /// you need to route through SSH (e.g. socks5://127.0.0.1:1080
    /// after `ssh -D 1080 <bastion>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,

    /// agent-browser `--proxy-bypass <hosts>` (comma-separated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_bypass: Option<String>,

    /// agent-browser `--args <flags>` — extra Chromium launch args.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_args: Option<String>,

    /// Override the agent-browser binary path. Default lookup:
    /// $AGENT_BROWSER_BIN -> "agent-browser" on $PATH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,

    /// Tunnel kind. Phase 1: only "direct" (or omitted) works.
    /// "ssh" returns a config error with the Phase 2 deferral note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

pub struct BrowserMcp;

#[async_trait]
impl McpTool for BrowserMcp {
    const NAME: &'static str = "browser_exec";
    const DESCRIPTION: &'static str =
        "Run one `agent-browser` CLI subcommand (https://github.com/vercel-labs/agent-browser) \
         and return its captured stdout / stderr / exit code. The browser daemon persists \
         between calls, so a sequence of calls with the same `session` share cookies / pages. \
         The `agent-browser` binary must be installed separately on the host running tools4a.";
    type Params = BrowserExecParams;

    async fn invoke(params: BrowserExecParams) -> Result<ExecutionResult> {
        let req = BrowserRequest {
            subcommand: params.subcommand,
            args: params.args,
            session: params.session,
            proxy: params.proxy,
            proxy_bypass: params.proxy_bypass,
            browser_args: params.browser_args,
            bin: params.bin.map(std::path::PathBuf::from),
        };

        let tunnel = build_tunnel_config(
            params.tunnel,
            params.ssh_jump,
            params.ssh_user,
            params.ssh_password,
            params.ssh_key_path,
            params.ssh_port,
        )?;

        BrowserOrchestrator::execute(req, tunnel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invoke_with_ssh_tunnel_surfaces_phase2_message() {
        let params = BrowserExecParams {
            subcommand: "snapshot".into(),
            args: Vec::new(),
            session: None,
            proxy: None,
            proxy_bypass: None,
            browser_args: None,
            bin: None,
            tunnel: Some(TunnelKind::Ssh),
            ssh_jump: Some(SshJumpInput::Single("bastion.example.com".into())),
            ssh_user: Some("admin".into()),
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let err = BrowserMcp::invoke(params).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Phase 2"), "got: {msg}");
    }
}
```

Verify the `SshJumpInput::Single` constructor matches the actual
`tools4a-core` shape — cross-check against an existing `mcp.rs`
(e.g. `crates/tools4a-mysql/src/mcp.rs` or
`crates/tools4a-http/src/mcp.rs`) before writing the test, and copy the
canonical form. The variant name may differ in core.

- [ ] **Step 2: Add `mcp` to `lib.rs`**

```rust
pub mod exec;
pub mod execute;
pub mod mcp;
pub mod orchestrator;
pub mod request;

pub use exec::{BrowserExec, BrowserOutput, output_to_result, resolve_bin};
pub use execute::execute;
pub use mcp::{BrowserExecParams, BrowserMcp};
pub use orchestrator::BrowserOrchestrator;
pub use request::BrowserRequest;
```

- [ ] **Step 3: Verify**

```bash
cargo test --package tools4a-browser
```

Expected: 7 prior + 1 new = 8 PASS.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(browser): BrowserExecParams + BrowserMcp impl McpTool

JSON params for browser_exec mirror the BrowserRequest fields plus
the standard tunnel/ssh_* fields (which Phase 1 only honors for
direct; ssh routes through to the orchestrator's Phase 2 deferral).
The leaf is now feature-complete pre-wiring; the bin layer is next.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: CLI subcommand `tools4a browser <subcommand> [args...]`

**Files:**
- Modify: `src/cli/args.rs`
- Modify: `src/cli/handler.rs`

- [ ] **Step 1: Add the `Browser` variant**

In `src/cli/args.rs`, append to the `Commands` enum (alphabetical
placement — between `Http` and `Clickhouse` if those exist, otherwise
just after the existing service variants; mirror the existing patterns
for ordering):

```rust
    /// Run an agent-browser CLI subcommand (browser automation)
    #[command(override_usage = "tools4a [GLOBAL OPTIONS] browser [OPTIONS] <SUBCOMMAND> [ARGS]...")]
    #[command(after_help = USAGE_LEGEND)]
    Browser {
        /// agent-browser subcommand (e.g. open, click, snapshot, batch, eval, screenshot).
        subcommand: String,

        /// Arguments passed after <SUBCOMMAND> verbatim to agent-browser.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,

        /// agent-browser --session NAME (isolates daemon state).
        #[arg(long, help_heading = "Browser")]
        session: Option<String>,

        /// agent-browser --proxy URL (e.g. socks5://127.0.0.1:1080).
        #[arg(long, help_heading = "Browser")]
        proxy: Option<String>,

        /// agent-browser --proxy-bypass HOSTS (comma-separated).
        #[arg(long = "proxy-bypass", help_heading = "Browser")]
        proxy_bypass: Option<String>,

        /// agent-browser --args FLAGS — extra Chromium launch arguments.
        #[arg(long = "browser-args", help_heading = "Browser")]
        browser_args: Option<String>,

        /// Override the agent-browser binary path. Defaults to
        /// $AGENT_BROWSER_BIN, then "agent-browser" on $PATH.
        #[arg(long, help_heading = "Browser")]
        bin: Option<std::path::PathBuf>,
    },
```

The `trailing_var_arg = true` + `allow_hyphen_values = true` combo lets
users write `tools4a browser open https://example.com --wait` without
clap trying to interpret `--wait` as one of tools4a's own flags. Match
the pattern used elsewhere in the codebase if there's an existing
convention (e.g. the way `tools4a ssh "<command>"` accepts arbitrary
shell strings).

- [ ] **Step 2: Wire the handler**

In `src/cli/handler.rs`, add a new match arm in `handle()` after the
existing `Commands::Ssh` arm:

```rust
            Some(Commands::Browser {
                subcommand,
                args,
                session,
                proxy,
                proxy_bypass,
                browser_args,
                bin,
            }) => {
                Self::execute_browser(
                    &cli,
                    subcommand,
                    args,
                    session,
                    proxy,
                    proxy_bypass,
                    browser_args,
                    bin,
                )
                .await
            }
```

Then add `execute_browser` to `impl CliHandler`:

```rust
    async fn execute_browser(
        cli: &Cli,
        subcommand: String,
        args: Vec<String>,
        session: Option<String>,
        proxy: Option<String>,
        proxy_bypass: Option<String>,
        browser_args: Option<String>,
        bin: Option<std::path::PathBuf>,
    ) -> Result<()> {
        let req = tools4a_browser::BrowserRequest {
            subcommand,
            args,
            session,
            proxy,
            proxy_bypass,
            browser_args,
            bin,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result =
            <tools4a_browser::BrowserOrchestrator as tools4a_core::Service>::execute(
                req,
                tunnel_config,
            )
            .await?;

        println!("{}", CliFormatter::format(&result));
        Ok(())
    }
```

Cross-check the dispatch idiom against `execute_ssh` (the closest
analogue) and match it — the call style above may need adjusting if
existing code uses `BrowserOrchestrator::execute(req, t)` directly
without the trait-qualified path.

- [ ] **Step 3: Verify**

```bash
cargo build
cargo run -q -- browser --help 2>&1 | head -20
```

Expected: help block starts with `Run an agent-browser CLI subcommand (browser automation)` and lists the Browser flag heading.

```bash
cargo run -q -- browser open https://example.com --proxy socks5://127.0.0.1:9999 2>&1 | head -5
```

Expected (assuming agent-browser is not installed on the CI box): an `Error: agent-browser binary not found ...` message routed through `main.rs`'s `eprintln!`. **Do not skip this manual verification** — it confirms the trailing-var-arg + handler wiring works end-to-end.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(cli): add 'browser <SUBCOMMAND> [ARGS]...' subcommand

Trailing-var-arg + allow_hyphen_values pattern so users can write
'tools4a browser open https://example.com --wait' without clap
trying to claim --wait. Flags --session / --proxy / --proxy-bypass
/ --browser-args / --bin sit under a 'Browser' help-heading; the
global --tunnel/--ssh-* flags route through cli_to_tunnel_config
(currently only direct is honored; ssh surfaces the Phase 2
deferral from the orchestrator).

Output uses the default CliFormatter table (exit_code / stdout /
stderr).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Wire `browser_exec` into the MCP server

**Files:**
- Modify: `src/mcp/server.rs`
- Modify: `tests/mcp_smoke.rs`

- [ ] **Step 1: Register the tool**

In `src/mcp/server.rs`:

a) Add the import alongside the existing leaf imports (alphabetical):

```rust
use tools4a_browser::{BrowserExecParams, BrowserMcp};
```

b) Append one `#[tool]` method to `impl ToolsMcpServer` (alongside the existing seven):

```rust
    #[tool(
        description = "Run one `agent-browser` CLI subcommand (browser automation via the external agent-browser binary). Returns exit_code, stdout, stderr. Pass the same `session` across calls to share daemon state (cookies, pages)."
    )]
    async fn browser_exec(
        &self,
        Parameters(params): Parameters<BrowserExecParams>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        into_call_result(BrowserMcp::invoke(params).await)
    }
```

c) Update the `with_instructions` server-info string to mention browser:

Before:
```
tools4a: unified MySQL / PostgreSQL / ClickHouse / Redis / MongoDB / \
HTTP / SSH tools with optional SSH tunneling.
```

After:
```
tools4a: unified MySQL / PostgreSQL / ClickHouse / Redis / MongoDB / \
HTTP / SSH / Browser tools with optional SSH tunneling (browser \
tunnel is direct-only in Phase 1; SOCKS via SSH lands in Phase 2).
```

- [ ] **Step 2: Update `tests/mcp_smoke.rs`**

Find the existing per-tool flags block (the one that tracks `found_mysql` / `found_redis` / `found_http` / etc.). Add `found_browser`:

```rust
            if line.contains("browser_exec") {
                found_browser = true;
            }
```

And the matching assertion:

```rust
    assert!(found_browser, "tools/list missing browser_exec");
```

Locate the existing variable declarations near the top of that test and add `let mut found_browser = false;` alongside the others.

- [ ] **Step 3: Verify**

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: all tests pass; clippy clean; `tests/mcp_smoke.rs` now lists `browser_exec`.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(mcp): browser_exec tool registered on the rmcp server

ToolsMcpServer now exposes the eighth tool: browser_exec. One-liner
dispatch through BrowserMcp::invoke. Server instructions updated to
mention Browser and the Phase 2 tunnel deferral.

mcp_smoke integration test asserts browser_exec shows up in
tools/list alongside the existing seven.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Plugin assets — `/browser` slash command + `browser-using` skill

**Files:**
- Create: `commands/browser.md`
- Create: `skills/browser-using/SKILL.md`

- [ ] **Step 1: `/browser` slash command**

Create `commands/browser.md`:

```markdown
---
name: browser
description: Run an agent-browser CLI subcommand through the tools4a `browser_exec` MCP tool.
argument-hint: <SUBCOMMAND> [ARGS...]
---

# /browser

Invoke the `browser_exec` MCP tool from the tools4a plugin:

```
$ARGUMENTS
```

## How to call it

1. **First token after `/browser` is the subcommand.** Common shapes:
   - `/browser open https://example.com`
   - `/browser click "selector=button#submit"`
   - `/browser snapshot`
   - `/browser batch -f script.txt`
   - `/browser cookies --session work`

2. **Translate flags into MCP tool params:**
   - First positional -> `subcommand`.
   - Everything after -> `args` array (one entry per token).
   - `--session NAME` -> `session`.
   - `--proxy URL` -> `proxy`.
   - `--proxy-bypass HOSTS` -> `proxy_bypass`.
   - `--browser-args FLAGS` -> `browser_args`.
   - `--bin PATH` -> `bin`.

3. **Pre-flight checks before destructive subcommands** (`fill` / `type` on prod forms, `click` on irreversible buttons, `eval` running JS, `network route` that rewrites traffic): confirm with the user.

4. **Read the result.** It's a 3-row ExecutionResult: `exit_code`, `stdout`, `stderr`. If `exit_code != 0`, surface `stderr` to the user. Otherwise show `stdout` (and parse it as JSON if the subcommand emits JSON — agent-browser's structured commands typically do).

## When something fails

- `Error::Config("agent-browser binary not found ...")` -> operator must install agent-browser separately. Don't try to install it automatically.
- `Error::Config("tunnel=ssh is not supported ... Phase 1 ...")` -> the user asked for `--tunnel=ssh` for the browser. Phase 2 will handle this; workaround in the error message (run `ssh -D 1080` + `--proxy socks5://127.0.0.1:1080`).
- Non-zero `exit_code` + agent-browser-specific message in `stderr` -> that's agent-browser's own diagnostic; read it and act on it (page not loaded, selector not found, etc.).
```

- [ ] **Step 2: `browser-using` skill**

Create `skills/browser-using/SKILL.md`:

```markdown
---
name: browser-using
description: Use when calling the `browser_exec` MCP tool from the tools4a plugin — explains the agent-browser daemon model, session reuse, common subcommands, --proxy passthrough for internal HTTPS, and Phase 2 SOCKS deferral.
---

# Using the `browser_exec` MCP tool

`tools4a` exposes `browser_exec`, a thin wrapper around the externally-installed [`agent-browser`](https://github.com/vercel-labs/agent-browser) CLI. tools4a does not embed a browser; agent-browser's daemon owns all state (pages / cookies / storage / authentication). Each call here is one short-lived CLI invocation against that persistent daemon.

## Pre-requisite

The operator must have installed `agent-browser` separately. tools4a will not download it. If you get `Error::Config("agent-browser binary not found ...")`, stop and ask the user to install it (`npm i -g agent-browser` or the upstream Rust build). Do NOT try to install it on the user's behalf.

## Tool input

```json
{
  "subcommand": "open",
  "args": ["https://example.com"],
  "session": "work",
  "proxy":   "socks5://127.0.0.1:1080",
  "proxy_bypass": "localhost,127.0.0.1",
  "browser_args": "--disable-gpu",
  "bin":     "/usr/local/bin/agent-browser"
}
```

`subcommand` and `args` are passed through verbatim — tools4a does not enumerate the agent-browser subcommand surface, so any new subcommand upstream works without a tools4a release.

## Session model

agent-browser's daemon persists between calls. Pass the same `session` across calls to share state:

```
{ "subcommand": "open",  "args": ["https://gmail.com"], "session": "personal" }
{ "subcommand": "fill",  "args": ["#email", "me@example.com"], "session": "personal" }
{ "subcommand": "click", "args": ["#next"], "session": "personal" }
```

Omitting `session` uses agent-browser's default session.

## Output shape

ExecutionResult (3 rows, `field`/`value` columns):

| field | value |
| --- | --- |
| `exit_code` | `0` |
| `stdout` | `<command output, often JSON>` |
| `stderr` | `<diagnostic output if any>` |

On success: show `stdout` (parse as JSON if it starts with `{` or `[`). On failure (`exit_code != 0`): show `stderr` — it carries agent-browser's structured error message (page not found, selector mismatch, etc.).

## Tunneling to internal HTTPS (Phase 1 workaround)

Phase 1 does NOT support `tunnel = "ssh"` for the browser — the existing single-port `direct-tcpip` tunnel doesn't fit a full browser. If the user needs to reach an internal HTTPS service through a bastion:

1. They run `ssh -D 1080 <bastion>` themselves (in a separate terminal / kept open).
2. Pass `"proxy": "socks5://127.0.0.1:1080"` to `browser_exec`.

Phase 2 will fold the SOCKS server into tools4a so `tunnel = "ssh"` works for browser too. If a user asks for `tunnel = "ssh"` today, you'll get an `Error::Config` whose message itself contains the workaround.

## Destructive subcommands — confirm first

| Subcommand | Destructive? | Confirm if... |
| --- | --- | --- |
| `open`, `back`, `forward`, `reload`, `snapshot`, `screenshot`, `get *`, `is *`, `cookies` (read) | No | — |
| `click` | Maybe | the page is prod / the button looks irreversible (`Submit`, `Delete`, `Pay`) |
| `fill`, `type` | Yes (mutates page state) | prod forms, especially with PII |
| `eval` | Yes (arbitrary JS) | always |
| `network route` / `unroute` | Yes (rewrites traffic) | always |
| `cookies` (write) / `storage` (write) | Yes | always |

When in doubt: prefer `snapshot` first to confirm what's on the page, then act.

## What this skill is NOT

- Not for embedding a browser inside tools4a itself — agent-browser is external.
- Not for installing or upgrading agent-browser — tells the user to run their own install if missing.
- Not for SOCKS tunneling through SSH (Phase 2). For now, instruct the user to set up `ssh -D` themselves and use `--proxy`.
- Not for `playwright` / `puppeteer` directly — those have their own MCP servers; this skill is specifically for the agent-browser surface.
```

- [ ] **Step 3: Verify the files exist**

```bash
ls commands/browser.md skills/browser-using/SKILL.md
```

Expected: both files present.

- [ ] **Step 4: Commit**

```bash
git add commands/browser.md skills/browser-using/
git commit -m "feat(plugin): /browser slash command + browser-using skill

- /browser <SUBCOMMAND> [ARGS...] — calls browser_exec with the
  subcommand + trailing args, plus optional session/proxy passthroughs.
- browser-using skill — daemon model, session reuse semantics,
  output shape (exit_code/stdout/stderr), Phase 1 SOCKS workaround
  via 'ssh -D' + --proxy, destructive-subcommand confirmation table.

Both call out that agent-browser is an external dependency the
operator installs separately; tools4a does not bundle or auto-install
it.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Documentation + final verification

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `AGENTS.md`

- [ ] **Step 1: README — Status + plugin assets**

In the Status / "implemented" list, add an entry for browser:

```markdown
- **Browser CLI mode** (`tools4a browser <SUBCOMMAND> [ARGS]...`) and `browser_exec` MCP tool — thin wrapper around the externally-installed [`agent-browser`](https://github.com/vercel-labs/agent-browser) binary. Phase 1: direct-only (no SSH tunnel); pass `--proxy socks5://127.0.0.1:1080` after running `ssh -D 1080 <bastion>` yourself for now.
```

In "Not yet implemented":

```markdown
- SSH tunnel for the browser tool (needs SOCKS5 routing; Phase 2)
```

In "Plugin assets":

```markdown
  - `browser_exec` — run an agent-browser CLI subcommand.
```

and

```markdown
  - `browser-using` — agent-browser daemon model, session reuse, Phase 1 SOCKS workaround.
```

and

```markdown
  - `/browser <SUBCOMMAND> [ARGS...]` — quick browser automation invocation.
```

- [ ] **Step 2: README — Usage example**

After the existing SSH example block, add:

````markdown
### Browser

```bash
# Pre-req: install agent-browser separately, e.g. `npm i -g agent-browser`.

# Open a URL in a named session, then click a selector
tools4a browser open https://example.com --session work
tools4a browser click "selector=#login" --session work

# Through a user-maintained SSH SOCKS proxy (Phase 1 workaround for SSH routing)
ssh -D 1080 bastion.example.com    # keep this terminal open
tools4a browser open https://internal-app.local --proxy socks5://127.0.0.1:1080
```
````

- [ ] **Step 3: CLAUDE.md / AGENTS.md updates**

Apply identical edits to both files.

a) **Project Overview lead sentence** — extend the service list to include Browser. Replace `MySQL, PostgreSQL, Redis, MongoDB, HTTP, and SSH` with `MySQL, PostgreSQL, ClickHouse, Redis, MongoDB, HTTP, SSH, and Browser` (match the existing exact phrasing). Update the leaf-crate parenthetical to add `tools4a-browser`.

b) **Module map** — add a row after `tools4a-ssh`:

```markdown
| `tools4a-browser` (lib) | Thin shell-out to the external `agent-browser` CLI (https://github.com/vercel-labs/agent-browser). `BrowserRequest`, `BrowserExec::run` (tokio::process::Command capturing stdout/stderr/exit_code), `output_to_result`, `execute(req)`, `BrowserOrchestrator: Service` (only Direct tunnel accepted in Phase 1; Ssh returns a Phase 2 deferral message), `BrowserMcp: McpTool` (name `"browser_exec"`). Owns no extra deps beyond core + tokio's `process` feature. |
```

c) **Phase boundaries** — append:

```markdown
- **Browser subcommand**: implemented in Phase 14 (Phase 1). `tools4a browser <SUBCOMMAND> [ARGS]...` and the `browser_exec` MCP tool both route through `BrowserOrchestrator::execute`, which shells out to the externally-installed `agent-browser` binary (https://github.com/vercel-labs/agent-browser) via `tokio::process::Command` and captures stdout / stderr / exit_code into the standard `ExecutionResult` shape. SSH tunnel is **NOT supported in Phase 1** — `tunnel=ssh` returns `Error::Config` with an explicit Phase 2 deferral message and a `ssh -D 1080` + `--proxy socks5://127.0.0.1:1080` workaround. Phase 2 will add a `SocksTunnel` to `tools4a-core` so `tunnel=ssh` works for the browser. Profile/YAML config for browser defaults is deferred (same Phase 1 simplification as HTTP / SSH-direct).
```

d) **Conventions worth knowing** — append:

```markdown
- **External-binary services**: `tools4a-browser` shells out to a binary the operator installs separately (no embedded browser). This is a third pattern alongside (a) typed-database services with Profile/YAML 3-layer merge (mysql/pgsql/clickhouse/redis/mongo) and (b) protocol-native services with no Profile/YAML (http, ssh-direct). When adding another external-binary service, copy the browser leaf shape: `BrowserRequest` + `BrowserExec::run` (process spawn + output capture) + `output_to_result` (3-row exit_code/stdout/stderr). The binary lookup pattern is `req.bin -> $<NAME>_BIN env -> "<name>" on $PATH`, with a clear `Error::Config` install-hint on ENOENT.
```

- [ ] **Step 4: Verify CLAUDE.md and AGENTS.md still match (modulo the cross-link)**

```bash
diff <(tail -n +5 CLAUDE.md) <(tail -n +5 AGENTS.md)
```

Expected: only the cross-link line + methodology trailer differ.

- [ ] **Step 5: Final workspace verification**

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
./target/release/tools4a browser --help | head -3
./target/release/tools4a mysql --help | head -3
./target/release/tools4a ssh --help | head -3
```

Expected: every step clean; `browser --help` shows `Run an agent-browser CLI subcommand (browser automation)`; regression checks confirm the other six subcommands still print their existing headers.

End-to-end smoke (only if agent-browser is installed on the box; OK to skip on CI):

```bash
agent-browser --version
./target/release/tools4a browser --help
./target/release/tools4a browser open https://example.com --session smoke
./target/release/tools4a browser snapshot --session smoke
```

Expected: the third command returns a non-empty `stdout` row containing an agent-browser snapshot payload, `exit_code = 0`.

- [ ] **Step 6: Commit**

```bash
git add README.md CLAUDE.md AGENTS.md
git commit -m "docs: document Phase 14 Phase 1 browser support

- README Status: browser tool added (Phase 1 — direct-only, agent-browser
  must be installed separately); Usage example showing the 'ssh -D' +
  --proxy workaround for SSH routing.
- CLAUDE.md / AGENTS.md: lead sentence + leaf-crate parenthetical
  extended; module map adds a tools4a-browser row; Phase boundaries
  record Phase 14 Phase 1 and the deferred SSH-tunnel work for Phase 2;
  conventions add a 'external-binary services' pattern note (browser as
  the prototype).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Summary

After Phase 14 Phase 1:

- `tools4a browser <SUBCOMMAND> [ARGS...]` works as a CLI subcommand with trailing-var-arg passthrough to agent-browser.
- The `browser_exec` MCP tool exposes the same surface to AI clients (subcommand + args + session + proxy passthroughs).
- Both share the orchestrator at `tools4a_browser::BrowserOrchestrator`.
- The new `tools4a-browser` leaf crate owns the `tokio::process` dep; no other workspace crate gains a dep.
- The plugin ships a `/browser` slash command + a `browser-using` skill.
- `tunnel=ssh` for browser surfaces an actionable Phase 2 deferral message with an inline `ssh -D` + `--proxy` workaround.
- Architecture remains: every CLI subcommand has a paired MCP tool; the leaf-crate vertical-slice shape (`request.rs` / `exec.rs` / `execute.rs` / `orchestrator.rs` / `mcp.rs`) holds for an eighth service.

**Deferred to Phase 14 Phase 2:**
- `tools4a_core::tunnel::SocksTunnel` (russh-based SOCKS5 listener on top of `channel_open_direct_tcpip`).
- `BrowserOrchestrator` honoring `TunnelConfig::Ssh` by building a `SocksTunnel`, binding to `127.0.0.1:<rand>`, and injecting `--proxy socks5://127.0.0.1:<rand>` into the agent-browser invocation.
- Skill update: remove the manual `ssh -D` workaround once Phase 2 ships.

**Deferred indefinitely (re-open if/when there's user demand):**
- Profile/YAML config for browser default `--proxy` / `--args` / `--session`.
- Bundling / version-pinning / auto-installing `agent-browser`.
- An MCP App UI resource for browser output (the SQL / HTTP tools have one; defer until usage patterns clarify what UI would be useful for heterogeneous browser stdout).
- Owning session lifecycle inside tools4a (the daemon does it).
