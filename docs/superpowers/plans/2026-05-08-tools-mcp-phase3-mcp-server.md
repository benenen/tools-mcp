# Tools MCP Phase 3: MCP Server Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When invoked with no subcommand, `tools-mcp` starts a Model Context Protocol server over stdio that exposes a `mysql_exec` tool — letting AI assistants run MySQL queries (with optional SSH tunneling) through the same execution path the CLI uses.

**Architecture:** Extract the existing CLI MySQL execution into a `core::mysql::execute` function that takes a fully-resolved `Config` and returns an `ExecutionResult`. The CLI handler keeps responsibility for arg parsing + table-formatted output. A new `mcp` module owns the rmcp server and the `mysql_exec` tool, which converts JSON params into a `Config` (loading profiles where requested) and delegates to the same core. Future Phase 4 subcommands (Redis, SSH-direct) plug in by adding their own core function + MCP tool — the architecture is "every CLI subcommand has a paired MCP tool over a shared core".

**Tech Stack:** [`rmcp`](https://crates.io/crates/rmcp) 1.6 (Anthropic's official Rust MCP SDK) + [`schemars`](https://crates.io/crates/schemars) (for tool input JSON schema). Reuses existing `core::mysql` (extracted in this plan), `tunnel::SshTunnel` / `DirectTunnel`, `connection::MySQLConnection`, `executor::MySQLExecutor`, `output::ExecutionResult`.

**Out of scope (Phase 4+):**
- Redis / SSH-direct subcommands and corresponding MCP tools
- HTTP/SSE transport (stdio only)
- Strict host-key checking, SSH key passphrases (carried over from Phase 2 deferrals)
- MCP resources, prompts, sampling — only tools in this phase

---

## File Structure

**Created:**
- `src/core/mod.rs` — module entry; re-exports `core::mysql::execute`.
- `src/core/mysql.rs` — `pub async fn execute(config: Config, query: &str) -> Result<ExecutionResult>` — resolves required fields, builds tunnel + connection, runs query, returns structured result. No printing or formatting.
- `src/mcp/mod.rs` — module entry; exports `serve_stdio`.
- `src/mcp/server.rs` — rmcp `ServerHandler` impl wiring tools.
- `src/mcp/tools.rs` — `MysqlExecParams` (input schema), `mysql_exec` tool body that converts params → `Config` → `core::mysql::execute` → JSON.

**Modified:**
- `Cargo.toml` — add `rmcp = "1.6"` (with stdio feature) and `schemars = "0.8"`.
- `src/lib.rs` — `pub mod core; pub mod mcp;`.
- `src/cli/handler.rs` — `execute_mysql` becomes a thin wrapper around `core::mysql::execute` + `CliFormatter::format` + `println!`.
- `src/main.rs` — no-subcommand branch: `mcp::serve_stdio().await` instead of the placeholder.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — document Phase 3.

---

## Task 1: Add rmcp + schemars deps and scaffold the `mcp` module

**Files:**
- Modify: `Cargo.toml`
- Create: `src/mcp/mod.rs`, `src/mcp/server.rs`, `src/mcp/tools.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add dependencies**

In `Cargo.toml` `[dependencies]`, add (alphabetical position; `rmcp` goes between `mysql_async` and `russh`, `schemars` between `serde_yml` and `tokio`):

```toml
rmcp = { version = "1.6", features = ["server", "transport-io"] }
schemars = "0.8"
```

If 1.6's feature names differ (e.g. `transport-stdio` vs `transport-io`), inspect `~/.cargo/registry/src/index.crates.io-*/rmcp-1.6.*/Cargo.toml` to find the correct flag. The required capabilities are: server side, stdio transport, and the tool macro/derive helpers (which may live in a `macros` feature or a re-exported `rmcp-macros` dep).

- [ ] **Step 2: Create empty mcp module files**

Create `src/mcp/mod.rs`:

```rust
mod server;
mod tools;

pub use server::serve_stdio;
```

Create `src/mcp/server.rs`:

```rust
use crate::error::{Error, Result};

/// Run the MCP server over stdio. Blocks until the client disconnects.
pub async fn serve_stdio() -> Result<()> {
    Err(Error::Connection(
        "MCP server not yet implemented".to_string(),
    ))
}
```

Create `src/mcp/tools.rs` (single comment placeholder; Task 3 fills it in):

```rust
// Tool definitions filled in by Task 3.
```

- [ ] **Step 3: Wire module in lib.rs**

In `src/lib.rs`, add `pub mod mcp;` (alphabetical position: after `error`, before `output`). Resulting lib.rs (preserving prior decls):

```rust
pub mod cli;
pub mod config;
pub mod connection;
pub mod error;
pub mod executor;
pub mod mcp;
pub mod output;
pub mod tunnel;

pub use error::{Error, Result};
```

(Keep declarations alphabetized.)

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: clean. rmcp will pull in a meaningful number of crates (tokio integration, schemars, etc.); first build will be slow.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/mcp/
git commit -m "feat(mcp): scaffold rmcp dependency and mcp module

Adds rmcp 1.6 with stdio transport and schemars 0.8 for tool input
schemas. Creates the empty mcp/{mod,server,tools}.rs scaffold with
serve_stdio() returning 'not yet implemented'. Subsequent tasks fill
in the tool definitions and main.rs wiring.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Extract `core::mysql::execute`

**Files:**
- Create: `src/core/mod.rs`, `src/core/mysql.rs`
- Modify: `src/cli/handler.rs` (the `execute_mysql` function only)
- Modify: `src/lib.rs`

- [ ] **Step 1: Write tests for `core::mysql::execute` failure on missing host/user**

Create `src/core/mysql.rs`:

```rust
use crate::config::{Config, TunnelConfig};
use crate::connection::{Connection, MySQLConnection};
use crate::error::{Error, Result};
use crate::executor::MySQLExecutor;
use crate::output::ExecutionResult;
use crate::tunnel::{DirectTunnel, SshTunnel, Tunnel};

/// Execute a single MySQL query against the connection described by `config`.
///
/// Errors if `config.host` or `config.user` is missing. Always tears down
/// the underlying connection (and SSH tunnel, if any) before returning,
/// regardless of whether the query succeeded.
pub async fn execute(config: Config, query: &str) -> Result<ExecutionResult> {
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
    let _ = conn.disconnect().await;
    exec_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_errors_on_missing_host() {
        let config = Config {
            user: Some("root".to_string()),
            ..Default::default()
        };
        let err = execute(config, "SELECT 1").await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_missing_user() {
        let config = Config {
            host: Some("localhost".to_string()),
            ..Default::default()
        };
        let err = execute(config, "SELECT 1").await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("user")));
    }
}
```

- [ ] **Step 2: Create core module entry**

Create `src/core/mod.rs`:

```rust
pub mod mysql;
```

- [ ] **Step 3: Wire `pub mod core;` into lib.rs**

In `src/lib.rs`, add `pub mod core;` between `pub mod connection;` and `pub mod error;`:

```rust
pub mod cli;
pub mod config;
pub mod connection;
pub mod core;
pub mod error;
pub mod executor;
pub mod mcp;
pub mod output;
pub mod tunnel;

pub use error::{Error, Result};
```

- [ ] **Step 4: Run new tests**

Run: `cargo test test_execute_errors_on_missing_host test_execute_errors_on_missing_user`
Expected: 2 PASS.

- [ ] **Step 5: Refactor `cli::handler` MySQL path to delegate**

In `src/cli/handler.rs`, replace `execute_mysql`'s body with:

```rust
async fn execute_mysql(query: &str, config: Config) -> Result<()> {
    let result = crate::core::mysql::execute(config, query).await?;
    let output = CliFormatter::format(&result);
    println!("{output}");
    Ok(())
}
```

Remove imports from `cli/handler.rs` that became unused:
- `MySQLConnection`, `MySQLExecutor`, `Connection`
- `DirectTunnel`, `SshTunnel`, `Tunnel`

`TunnelConfig` may still be referenced by `cli_to_tunnel_config` — keep it.

Run `cargo build` after the import cleanup; the compiler will tell you exactly which imports became unused.

- [ ] **Step 6: Verify full test suite**

Run: `cargo test`
Expected: 20 tests pass (18 prior + 2 new).

- [ ] **Step 7: Commit**

```bash
git add src/core/ src/cli/handler.rs src/lib.rs
git commit -m "refactor(core): extract MySQL execution into core::mysql::execute

The CLI handler used to embed query execution + connection teardown
directly. That logic now lives in core::mysql::execute which takes a
resolved Config and returns ExecutionResult — no printing, no
formatting. The CLI handler keeps the parse-args + format-output
responsibility.

This sets up Phase 3's MCP tool to delegate to the same core, so CLI
and MCP go through identical execution + teardown paths.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Define MCP tool input schema and `mysql_exec` body

**Files:**
- Modify: `src/mcp/tools.rs`

- [ ] **Step 1: Define input schema struct**

Replace `src/mcp/tools.rs` with:

```rust
use crate::config::{Config, ConfigLoader, ServiceType, TunnelConfig};
use crate::core::mysql;
use crate::error::{Error, Result};
use crate::output::ExecutionResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// JSON parameters for the `mysql_exec` MCP tool. Mirrors the CLI's
/// `mysql` subcommand args plus the global tunnel/config flags, so an
/// AI assistant can invoke any MySQL query the CLI can run.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MysqlExecParams {
    /// SQL query to execute.
    pub query: String,

    /// MySQL host (overrides profile / yaml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// MySQL port (default 3306).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// MySQL user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// MySQL password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Database name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    /// Profile name from ~/.config/tools-mcp/config.toml (or
    /// $XDG_CONFIG_HOME/tools-mcp/config.toml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Path to a YAML config file to load before applying explicit fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s). Comma-separated string for multi-hop, or a JSON array.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user (used when `tunnel = "ssh"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TunnelKind {
    Direct,
    Ssh,
}

/// Accepts either a single host string, a comma-separated string, or a JSON array.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SshJumpInput {
    Single(String),
    Multiple(Vec<String>),
}

impl SshJumpInput {
    pub fn into_jumps(self) -> Vec<String> {
        match self {
            SshJumpInput::Single(s) => s
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
            SshJumpInput::Multiple(v) => v.into_iter().filter(|s| !s.is_empty()).collect(),
        }
    }
}
```

- [ ] **Step 2: Build params -> Config helper**

Append to `src/mcp/tools.rs`:

```rust
/// Convert the JSON params into a fully-resolved `Config`, applying the
/// same priority order as the CLI: TOML profile (lowest) -> YAML file ->
/// explicit MCP fields (highest).
fn params_to_config(p: &MysqlExecParams) -> Result<Config> {
    let mut configs: Vec<Config> = Vec::new();

    if let Some(profile_name) = &p.profile {
        let toml_config = ConfigLoader::load_default_toml()?.ok_or_else(|| {
            Error::Config(format!(
                "profile '{}' requested but no ~/.config/tools-mcp/config.toml found",
                profile_name
            ))
        })?;
        let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
            Error::Config(format!("profile '{}' not found in config.toml", profile_name))
        })?;
        configs.push(profile_to_config(profile_cfg));
    }

    if let Some(path) = p.config.as_deref() {
        configs.push(ConfigLoader::load_yaml_file(path)?);
    }

    let tunnel_config = build_tunnel_config(p)?;
    configs.push(Config {
        service_type: Some(ServiceType::Mysql),
        host: p.host.clone(),
        port: p.port,
        user: p.user.clone(),
        password: p.password.clone(),
        database: p.database.clone(),
        key_path: None,
        tunnel: tunnel_config,
    });

    Ok(crate::config::ConfigMerger::merge_multiple(configs))
}

fn profile_to_config(profile: &crate::config::Profile) -> Config {
    Config {
        service_type: Some(profile.service_type.clone()),
        host: profile.host.clone(),
        port: profile.port,
        user: profile.user.clone(),
        password: profile.password.clone(),
        database: profile.database.clone(),
        key_path: profile.key_path.clone(),
        tunnel: profile.tunnel.clone(),
    }
}

fn build_tunnel_config(p: &MysqlExecParams) -> Result<Option<TunnelConfig>> {
    let Some(kind) = &p.tunnel else {
        return Ok(None);
    };
    match kind {
        TunnelKind::Direct => {
            let stray = p.ssh_jump.is_some()
                || p.ssh_user.is_some()
                || p.ssh_password.is_some()
                || p.ssh_key_path.is_some()
                || p.ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = p
                .ssh_jump
                .clone()
                .map(SshJumpInput::into_jumps)
                .ok_or_else(|| {
                    Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
                })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = p.ssh_user.clone().ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password: p.ssh_password.clone(),
                ssh_key_path: p.ssh_key_path.clone(),
                ssh_port: p.ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the mysql_exec tool: params in, structured
/// result out. The MCP server wraps this with JSON-RPC plumbing.
pub async fn mysql_exec(params: MysqlExecParams) -> Result<ExecutionResult> {
    let query = params.query.clone();
    let config = params_to_config(&params)?;
    mysql::execute(config, &query).await
}
```

- [ ] **Step 3: Tests for the params -> Config conversion**

Append to the bottom of `src/mcp/tools.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn empty_params() -> MysqlExecParams {
        MysqlExecParams {
            query: "SELECT 1".to_string(),
            host: None,
            port: None,
            user: None,
            password: None,
            database: None,
            profile: None,
            config: None,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        }
    }

    #[test]
    fn test_explicit_fields_become_config() {
        let p = MysqlExecParams {
            host: Some("db.example.com".into()),
            port: Some(3307),
            user: Some("alice".into()),
            ..empty_params()
        };
        let cfg = params_to_config(&p).unwrap();
        assert_eq!(cfg.host.as_deref(), Some("db.example.com"));
        assert_eq!(cfg.port, Some(3307));
        assert_eq!(cfg.user.as_deref(), Some("alice"));
    }

    #[test]
    fn test_tunnel_ssh_with_string_jump_splits_commas() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Ssh),
            ssh_jump: Some(SshJumpInput::Single("b1.com,b2.com".into())),
            ssh_user: Some("admin".into()),
            ..empty_params()
        };
        let cfg = params_to_config(&p).unwrap();
        match cfg.tunnel {
            Some(TunnelConfig::Ssh { ssh_jumps, .. }) => {
                assert_eq!(ssh_jumps, vec!["b1.com".to_string(), "b2.com".to_string()]);
            }
            other => panic!("expected Ssh tunnel, got {other:?}"),
        }
    }

    #[test]
    fn test_tunnel_ssh_with_array_jump() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Ssh),
            ssh_jump: Some(SshJumpInput::Multiple(vec!["b1".into(), "b2".into()])),
            ssh_user: Some("admin".into()),
            ..empty_params()
        };
        let cfg = params_to_config(&p).unwrap();
        match cfg.tunnel {
            Some(TunnelConfig::Ssh { ssh_jumps, .. }) => {
                assert_eq!(ssh_jumps, vec!["b1".to_string(), "b2".to_string()]);
            }
            other => panic!("expected Ssh tunnel, got {other:?}"),
        }
    }

    #[test]
    fn test_tunnel_direct_with_stray_ssh_field_errors() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Direct),
            ssh_jump: Some(SshJumpInput::Single("bastion".into())),
            ..empty_params()
        };
        let err = params_to_config(&p).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("ssh_*")));
    }

    #[test]
    fn test_tunnel_ssh_without_jump_errors() {
        let p = MysqlExecParams {
            tunnel: Some(TunnelKind::Ssh),
            ssh_user: Some("admin".into()),
            ..empty_params()
        };
        let err = params_to_config(&p).unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("ssh_jump")));
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib mcp::tools`
Expected: 5 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/tools.rs
git commit -m "feat(mcp): mysql_exec tool params + Config builder

Defines MysqlExecParams (JSON schema for the tool input) and the
params-to-Config conversion that mirrors the CLI's three-layer merge
(TOML profile -> YAML file -> explicit fields). The mysql_exec entry
point delegates to core::mysql::execute, so CLI and MCP go through
identical execution and teardown.

The ssh_jump field accepts either a single string (comma-separated
for multi-hop) or a JSON array, matching the CLI ergonomics.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: rmcp `ServerHandler` registering `mysql_exec`

**Files:**
- Modify: `src/mcp/server.rs`
- Modify: `src/mcp/mod.rs` (re-export tools module if needed)

This task wires the rmcp protocol layer. The exact API shape of rmcp 1.6 may differ from what's shown here — there are typically two patterns: (a) the high-level `#[tool]` macro that auto-derives everything from a `MysqlExecParams` struct, or (b) manual `ServerHandler::list_tools` + `call_tool` implementations. Try (a) first because it eliminates boilerplate; fall back to (b) if the macro doesn't fit cleanly.

- [ ] **Step 1: Replace `serve_stdio` with the real implementation**

Run `cargo doc --no-deps -p rmcp --open` (or read `~/.cargo/registry/src/.../rmcp-1.6.*/src/`) to find the canonical pattern. The most common 1.6 idiom is:

```rust
use crate::error::Result;
use crate::mcp::tools::{mysql_exec, MysqlExecParams};
use rmcp::handler::server::tool::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::service::ServiceExt;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};

#[derive(Debug, Clone)]
pub struct ToolsMcpServer {
    tool_router: rmcp::handler::server::tool::ToolRouter<Self>,
}

#[tool_router]
impl ToolsMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Execute a MySQL query, optionally through an SSH tunnel.
    #[tool(description = "Execute a MySQL query, optionally through an SSH jump host. Same connection options as the `tools-mcp mysql` CLI subcommand.")]
    async fn mysql_exec(
        &self,
        Parameters(params): Parameters<MysqlExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match mysql_exec(params).await {
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
}

#[tool_handler]
impl ServerHandler for ToolsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "tools-mcp: MySQL query execution with optional SSH tunneling. \
                 Use the mysql_exec tool. Connection params can come from a TOML \
                 profile (~/.config/tools-mcp/config.toml), a YAML file, or be \
                 supplied directly in the tool call."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Run the MCP server over stdio. Blocks until the client disconnects.
pub async fn serve_stdio() -> Result<()> {
    let server = ToolsMcpServer::new();
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| crate::Error::Connection(format!("MCP server start failed: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| crate::Error::Connection(format!("MCP server error: {e}")))?;
    Ok(())
}
```

This snippet uses **all the most likely names** from rmcp 1.6:
- `#[tool_router]` macro generates the dispatch table
- `#[tool(description = "...")]` macro registers a function as a tool
- `#[tool_handler]` macro implements `ServerHandler::call_tool` etc.
- `Parameters<T>` extractor wraps the deserialized JSON
- `ServiceExt::serve(transport)` to bind the transport
- `rmcp::transport::stdio()` for the stdio transport
- `ServerCapabilities::builder().enable_tools().build()` to declare tool support

If any of those names differ in 1.6, adapt — the structural pattern (one struct, one tool function, one ServerHandler impl, one stdio transport, one wait loop) is what matters. If the macros don't exist at all, fall back to a manual `impl ServerHandler` implementing `list_tools` and `call_tool` by hand.

- [ ] **Step 2: Adjust `tools.rs` exports if needed**

The `mcp::server::ToolsMcpServer` references `mysql_exec` and `MysqlExecParams` from `tools.rs`. Both are already `pub` from Task 3 — but the `tools` module is declared private in `mod.rs`. Make the module public:

Update `src/mcp/mod.rs`:

```rust
mod server;
pub mod tools;

pub use server::serve_stdio;
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: clean. If rmcp macros generate code that fails to compile, that's the API drift moment — read the macro's actual output (cargo expand) or the rmcp examples in the published source tree.

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: 25 pass (20 prior + 5 mcp::tools tests). The new server module has no tests yet — that's Task 6.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/server.rs src/mcp/mod.rs
git commit -m "feat(mcp): rmcp server with mysql_exec tool registered

ToolsMcpServer is a rmcp ServerHandler that exposes a single tool,
mysql_exec, dispatching JSON params to mcp::tools::mysql_exec which
in turn delegates to core::mysql::execute. serve_stdio() binds the
stdio transport and waits for client termination.

The tool returns the ExecutionResult as pretty-printed JSON in a text
content block. Errors come back as an MCP error result rather than
crashing the server, so an AI client gets actionable feedback.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Wire MCP startup in `main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace placeholder branch**

Current `src/main.rs`:

```rust
use clap::Parser;
use tools_mcp::cli::{Cli, CliHandler};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.command.is_none() {
        eprintln!("MCP mode not yet implemented. Use a subcommand (mysql) for CLI mode.");
        std::process::exit(1);
    }

    if let Err(e) = CliHandler::handle(cli).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
```

Replace with:

```rust
use clap::Parser;
use tools_mcp::cli::{Cli, CliHandler};
use tools_mcp::mcp;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = if cli.command.is_none() {
        // No subcommand -> run MCP server over stdio.
        mcp::serve_stdio().await
    } else {
        CliHandler::handle(cli).await
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
```

`mcp::serve_stdio` is `pub use`-exported from `src/mcp/mod.rs`.

- [ ] **Step 2: Verify build + tests**

Run: `cargo build && cargo test`
Expected: clean build; 25 tests pass.

- [ ] **Step 3: Sanity-check CLI path still works**

Run: `cargo run -- mysql --help 2>&1 | head -3`
Expected:

```
Execute a MySQL query

Usage: tools-mcp [GLOBAL OPTIONS] mysql [OPTIONS] <QUERY>
```

(Same as before — CLI mode unaffected.)

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): start MCP server when no subcommand is given

Replaces the placeholder error with a real MCP stdio server. CLI
mode (any subcommand) is unchanged. Running 'tools-mcp' bare now
binds stdio and serves the MCP protocol — ready for AI clients.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Integration smoke test against the running MCP server

**Files:**
- Create: `tests/mcp_smoke.rs`

This task drives the binary as a subprocess, sends a JSON-RPC `initialize` + `tools/list` exchange over its stdin, and verifies that `mysql_exec` is in the response. It avoids needing a real MySQL or SSH server.

- [ ] **Step 1: Write the integration test**

Create `tests/mcp_smoke.rs`:

```rust
//! End-to-end smoke test that runs the binary with no subcommand
//! (which boots the MCP server) and exchanges a minimal JSON-RPC
//! handshake over its stdio. Verifies that `mysql_exec` shows up in
//! `tools/list`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tools-mcp"))
}

#[test]
fn test_mcp_lists_mysql_exec_tool() {
    let mut child = Command::new(binary_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn tools-mcp");

    let mut stdin = child.stdin.take().expect("no stdin");
    let stdout = child.stdout.take().expect("no stdout");
    let mut reader = BufReader::new(stdout);

    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke-test","version":"0.0.1"}}}"#;
    writeln!(stdin, "{initialize}").unwrap();

    let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    writeln!(stdin, "{initialized}").unwrap();

    let list_tools = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    writeln!(stdin, "{list_tools}").unwrap();
    stdin.flush().unwrap();

    let mut found_tool = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        if n == 0 {
            break;
        }
        if line.contains("\"id\":2") && line.contains("mysql_exec") {
            found_tool = true;
            break;
        }
    }

    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();

    assert!(
        found_tool,
        "tools/list response did not contain mysql_exec within 10s"
    );
}

trait WaitTimeoutExt {
    fn wait_timeout(&mut self, dur: Duration) -> Option<std::process::ExitStatus>;
}

impl WaitTimeoutExt for std::process::Child {
    fn wait_timeout(&mut self, dur: Duration) -> Option<std::process::ExitStatus> {
        let deadline = std::time::Instant::now() + dur;
        while std::time::Instant::now() < deadline {
            match self.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return None,
            }
        }
        None
    }
}
```

Notes:
- `CARGO_BIN_EXE_tools-mcp` is set automatically for integration tests.
- The MCP protocol version `"2024-11-05"` is current stable; rmcp 1.6 should accept it. If not, adjust to whatever rmcp expects.
- `notifications/initialized` is the spec-required notification after the initialize response.
- The reader reads line-by-line because rmcp's stdio transport uses newline-delimited JSON-RPC.

- [ ] **Step 2: Run the integration test**

Run: `cargo test --test mcp_smoke`
Expected: PASS within ~1s. If it hangs, the protocol version may be wrong, or rmcp's stdio framing uses Content-Length headers instead of newlines (inspect rmcp's stdio transport source).

- [ ] **Step 3: Verify full test suite**

Run: `cargo test`
Expected: all tests pass (Phase 2's 18 + Phase 3's mcp::tools tests + 2 core::mysql + 1 mcp smoke).

- [ ] **Step 4: Commit**

```bash
git add tests/mcp_smoke.rs
git commit -m "test(mcp): smoke test that mysql_exec shows up in tools/list

Drives the binary as a subprocess, exchanges JSON-RPC initialize +
tools/list over stdio, and asserts that mysql_exec is present. No
real MySQL or SSH server required.

Catches regressions in: rmcp wiring, the #[tool] macro registration,
JSON-RPC framing, and the no-subcommand -> MCP startup path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: Update README + CLAUDE.md + AGENTS.md

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: README Status section**

Replace the existing `## Status` section so MCP no longer appears under "Not yet implemented":

```markdown
## Status

This is the Phase 3 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`)
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
- Direct connection (`--tunnel=direct` or no `--tunnel` flag)
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth (`--ssh-password` / `--ssh-key-path`).
  Host keys are accepted with a fingerprint warning (a future phase will add strict checking).
- **MCP server mode** (run `tools-mcp` with no subcommand): exposes a `mysql_exec`
  tool to AI clients over stdio. Same connection / tunnel / profile options as the CLI.

Not yet implemented:
- Redis support
- SSH direct connection (`tools-mcp ssh ...`)
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
- HTTP/SSE MCP transport
```

Add a new `### MCP Server` subsection between the MySQL Usage examples and the Configuration section:

```markdown
### MCP Server

Run `tools-mcp` with no subcommand to start an MCP server over stdio:

```bash
tools-mcp
```

It exposes one tool, `mysql_exec`, with the same parameters as the CLI's
`mysql` subcommand (host/port/user/password/database/profile + tunnel/ssh_*).
AI clients (Claude Desktop, Cursor, etc.) can call this tool to run MySQL
queries through SSH jump hosts.

Example MCP configuration entry (e.g. for Claude Desktop):

```json
{
  "mcpServers": {
    "tools-mcp": {
      "command": "/usr/local/bin/tools-mcp"
    }
  }
}
```
```

- [ ] **Step 2: CLAUDE.md and AGENTS.md updates**

Both files must end up identical apart from the cross-link header. In **both** files apply these edits:

**Project Overview** — update the lead sentence:

Before:
```markdown
`tools-mcp` is a Rust CLI / future MCP server for SSH, MySQL, and Redis. **Phase 2 (current) implements MySQL CLI mode with SSH tunnel support**; Redis, SSH direct, and MCP server mode are explicit phase boundaries (see below).
```

After:
```markdown
`tools-mcp` is a Rust CLI + MCP server for SSH, MySQL, and Redis. **Phase 3 (current) implements MySQL CLI mode + MCP server mode with the `mysql_exec` tool**; Redis and SSH direct are explicit phase boundaries (see below).
```

**Module map** — add two new rows after `tunnel::...`:

```markdown
| `core::mysql` | `execute(config, query) -> ExecutionResult` — the shared MySQL execution path. CLI handler and MCP tool both delegate here so teardown semantics are identical. |
| `mcp::{server,tools}` | rmcp-based stdio server. Single `mysql_exec` tool delegates to `core::mysql::execute`. Tool params mirror the CLI's `mysql` subcommand args + global tunnel/config flags. |
```

**Phase boundaries** — replace the MCP entry. Before:

```markdown
- **MCP server mode**: triggered when no subcommand is given; `main.rs` prints a placeholder and exits 1. (Unchanged from Phase 1.)
```

After:

```markdown
- **MCP server mode**: implemented in Phase 3. `main.rs` runs `mcp::serve_stdio` when no subcommand is given. Single tool `mysql_exec` (in `mcp::tools`) routes to `core::mysql::execute` — same execution path as `tools-mcp mysql "..."`.
- **Redis / SSH-direct subcommands**: not yet implemented. When added, mirror the existing pattern: a `core::<service>` execution function, a CLI subcommand under `cli::Commands`, and an MCP tool in `mcp::tools` that delegates to the core. CLI and MCP must share the core; never duplicate execution logic in MCP land.
```

**Conventions worth knowing** — add a new bullet at the bottom:

```markdown
- **CLI <-> MCP parity**: every CLI subcommand has (or will have) a paired MCP tool, and both delegate to the same `core::<service>` function. When adding a new subcommand, write the core function first, then wire CLI and MCP on top — never embed business logic in either presentation layer.
```

- [ ] **Step 3: Verify both files differ only on the cross-link**

Run: `diff <(tail -n +5 CLAUDE.md) <(tail -n +5 AGENTS.md)`
Expected: only the cross-link blockquote line and the pre-existing methodology-line diff.

- [ ] **Step 4: Build + test**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all clean.

- [ ] **Step 5: Commit**

```bash
git add README.md CLAUDE.md AGENTS.md
git commit -m "docs: document Phase 3 MCP server mode

- README Status: MCP shipped, CLI <-> MCP parity through core::mysql.
- Add 'MCP Server' usage section with Claude Desktop config example.
- CLAUDE.md/AGENTS.md: update Project Overview, add core::mysql and
  mcp module-map rows, refresh Phase boundaries, document the
  CLI-MCP parity convention so future agents preserve it.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Phase 3 final verification

**Files:** none (verification only).

- [ ] **Step 1: Full test suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 2: Lint**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Format**

Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 4: Release build**

Run: `cargo build --release`
Expected: `target/release/tools-mcp` produced.

- [ ] **Step 5: CLI smoke (regression)**

Run: `./target/release/tools-mcp mysql --help | head -3`
Expected:

```
Execute a MySQL query

Usage: tools-mcp [GLOBAL OPTIONS] mysql [OPTIONS] <QUERY>
```

- [ ] **Step 6: MCP smoke (manual, optional)**

If a real MCP client is available (Claude Desktop, mcp-cli, etc.), wire `tools-mcp` as an MCP server and call `mysql_exec` with `{"query": "SELECT 1", "host": "...", "user": "...", "password": "..."}`. Expect a JSON ExecutionResult in the tool response. If no client is available, this is covered by the smoke test in Task 6.

- [ ] **Step 7 (optional): Phase 3 roll-up commit**

If anything was left unstaged, sweep it. Otherwise skip.

---

## Summary

After Phase 3:

- `tools-mcp` (no args) starts an MCP server over stdio.
- The server exposes `mysql_exec` with the same parameter shape as the CLI's `mysql` subcommand — including SSH tunneling, profiles, and YAML configs.
- CLI handler and MCP tool both go through `core::mysql::execute`; teardown semantics are identical.
- Architecture is ready for Phase 4: adding Redis or SSH-direct means a new `core::<service>` function + a `cli::Commands` variant + an `mcp::tools::<tool>` registration.

**Deferred to Phase 4+:** Redis subcommand + tool, SSH-direct subcommand + tool, HTTP/SSE MCP transport, MCP resources/prompts, strict host-key checking, SSH key passphrases, per-hop auth overrides.
