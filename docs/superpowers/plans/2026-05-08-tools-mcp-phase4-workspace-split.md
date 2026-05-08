# Tools MCP Phase 4: Workspace Split (`tools-mcp-core` + `tools-mcp-mysql`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the single-crate repo into a Cargo workspace with three crates — `tools-mcp-core` (traits + shared types), `tools-mcp-mysql` (MySQL connection/executor + thin entry function), and `tools-mcp` (the existing binary). Pure refactor: no behavior change, all 26 tests still pass.

**Architecture:**
- `tools-mcp-core` is the dependency floor: `Tunnel` + `Connection` async traits, `TunnelEndpoint`, `Error`/`Result` (with only IO + generic Service variants — no service-specific deps), `ExecutionResult`. Sole external dep: `async-trait`.
- `tools-mcp-mysql` depends on `tools-mcp-core` and provides the MySQL primitives — `MySQLConnection` (impl `core::Connection`), `MySQLExecutor`, plus a thin `execute(tunnel, params, query)` entry function. Sole MySQL dep: `mysql_async`.
- `tools-mcp` (the bin) depends on both libs. It owns presentation-layer code: clap CLI, rmcp MCP server, `Config` types + 3-layer merge, `DirectTunnel`/`SshTunnel` impls (russh stays here), `CliFormatter`. Its `core::mysql::execute(Config, &str)` becomes a thin orchestrator: validates required fields, builds a tunnel from `Config.tunnel`, calls into `tools_mcp_mysql::execute`.

**Tech stack unchanged.** Only file moves + manifest splits.

**Out of scope:**
- No new functionality. No clippy/style cleanup beyond what the refactor strictly requires.
- `DirectTunnel`/`SshTunnel` stay in `tools-mcp` per the user's chosen narrow `tools-mcp-core` scope (no russh in core).

---

## File Structure (after refactor)

```
tools-mcp/
├── Cargo.toml                          ← workspace manifest (no [package])
├── Cargo.lock
├── .claude-plugin/, .mcp.json, README.md, CLAUDE.md, AGENTS.md, Makefile, docs/
└── crates/
    ├── tools-mcp-core/
    │   ├── Cargo.toml                  ← lib, async-trait only
    │   └── src/
    │       └── lib.rs                  ← Error, Result, Tunnel, Connection, TunnelEndpoint, ExecutionResult
    ├── tools-mcp-mysql/
    │   ├── Cargo.toml                  ← lib, depends on tools-mcp-core + mysql_async + async-trait
    │   └── src/
    │       ├── lib.rs                  ← re-exports
    │       ├── connection.rs           ← MySQLConnection (impl core::Connection)
    │       ├── executor.rs             ← MySQLExecutor + value_to_string
    │       └── execute.rs              ← execute(tunnel, params, query) entry
    └── tools-mcp/
        ├── Cargo.toml                  ← bin, depends on tools-mcp-core + tools-mcp-mysql + clap/rmcp/russh/etc.
        ├── src/
        │   ├── main.rs                 (unchanged content; just a different crate root)
        │   ├── lib.rs
        │   ├── cli/                    (unchanged)
        │   ├── mcp/                    (unchanged)
        │   ├── config/                 (unchanged — Config types live with the bin)
        │   ├── core/mysql.rs           (orchestrator: Config → tunnel + mysql params → tools_mcp_mysql::execute)
        │   ├── tunnel/                 (DirectTunnel + SshTunnel stay; russh stays)
        │   └── output/cli.rs           (CliFormatter; uses tools_mcp_core::ExecutionResult)
        └── tests/
            ├── config_tests.rs
            └── mcp_smoke.rs
```

What moves where:
- `src/error.rs` → `crates/tools-mcp-core/src/lib.rs` (the `Yaml`/`Toml`/`Mysql` variants drop; replaced by `Service(String)` for any wrapped service-specific error. Bin's CLI/config code converts `serde_yml::Error` / `toml::de::Error` to `Error::Config(format!(...))` at the call site, so core doesn't depend on those crates.)
- `src/tunnel/traits.rs` → `crates/tools-mcp-core/src/lib.rs` (`Tunnel`, `TunnelEndpoint`)
- `src/connection/traits.rs` → `crates/tools-mcp-core/src/lib.rs` (`Connection`)
- `src/output/types.rs` → `crates/tools-mcp-core/src/lib.rs` (`ExecutionResult`; the `Serialize`/`Deserialize` derives stay so MCP can JSON-serialize without changes)
- `src/connection/mysql.rs` → `crates/tools-mcp-mysql/src/connection.rs`
- `src/executor/mysql.rs` → `crates/tools-mcp-mysql/src/executor.rs`
- `src/core/mysql.rs` body splits: tunnel construction + Config validation stay in bin; the "have tunnel + params, run query" half becomes `tools-mcp-mysql::execute(...)`.
- Everything else stays in the bin crate.

---

## Task 1: Bootstrap workspace skeleton

**Files:**
- Create: `Cargo.toml.workspace.tmp` (intermediate) — actually we modify `Cargo.toml` and `Cargo.lock` as part of the move
- Create: `crates/tools-mcp/Cargo.toml`
- Create: `crates/tools-mcp-core/Cargo.toml` + `crates/tools-mcp-core/src/lib.rs` (empty stub)
- Create: `crates/tools-mcp-mysql/Cargo.toml` + `crates/tools-mcp-mysql/src/lib.rs` (empty stub)
- Move: top-level `src/`, `tests/`, `Makefile` is unchanged but `cargo` invocations now go through workspace
- Move: top-level `Cargo.toml` `[package]`/`[dependencies]`/etc. to `crates/tools-mcp/Cargo.toml`

This task is the most fragile because it changes the build root. Done in one commit so that "cargo build" + tests stay green throughout.

- [ ] **Step 1: Move existing crate into `crates/tools-mcp/`**

```bash
cd /root/workspace/master/tools-mcp
git mv src crates/tools-mcp-tmp-src    # avoid name collision while moving
git mv tests crates/tools-mcp-tmp-tests
mkdir -p crates/tools-mcp
git mv crates/tools-mcp-tmp-src crates/tools-mcp/src
git mv crates/tools-mcp-tmp-tests crates/tools-mcp/tests
```

(Using a tmp name avoids the case where `git mv src crates/tools-mcp/src` complains about target-dir-exists.)

- [ ] **Step 2: Create `crates/tools-mcp/Cargo.toml`**

This is the existing top-level `Cargo.toml` minus the `[workspace]` we'll add at the root. Copy its contents:

```toml
[package]
name = "tools-mcp"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "tools-mcp"
path = "src/main.rs"

[lib]
name = "tools_mcp"
path = "src/lib.rs"

[dependencies]
async-trait = "0.1"
clap = { version = "4.5", features = ["derive"] }
comfy-table = "7.1"
mysql_async = "0.34"
rmcp = { version = "1.6", features = ["server", "transport-io"] }
russh = "0.46"
schemars = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yml = "0.0.12"
tokio = { version = "1.40", features = ["full"] }
toml = "0.8"

[dev-dependencies]
tempfile = "3.12"
```

(Copy the EXACT dep list from the current root `Cargo.toml` — any drift breaks tests. Verify with `diff` before deleting the original.)

- [ ] **Step 3: Replace the top-level `Cargo.toml` with a workspace manifest**

Overwrite `/root/workspace/master/tools-mcp/Cargo.toml` with:

```toml
[workspace]
resolver = "3"
members = [
    "crates/tools-mcp",
    "crates/tools-mcp-core",
    "crates/tools-mcp-mysql",
]
```

(`resolver = "3"` matches edition 2024. If `cargo` complains, fall back to `"2"`.)

- [ ] **Step 4: Create empty `tools-mcp-core` and `tools-mcp-mysql` placeholders**

`crates/tools-mcp-core/Cargo.toml`:

```toml
[package]
name = "tools-mcp-core"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
```

`crates/tools-mcp-core/src/lib.rs`:

```rust
//! Core traits and shared types for the tools-mcp workspace.
//!
//! This crate is the dependency floor: only `async-trait` and `serde`.
//! Service-specific code (MySQL, SSH, etc.) lives in higher crates.
```

`crates/tools-mcp-mysql/Cargo.toml`:

```toml
[package]
name = "tools-mcp-mysql"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
mysql_async = "0.34"
tools-mcp-core = { path = "../tools-mcp-core" }
```

`crates/tools-mcp-mysql/src/lib.rs`:

```rust
//! MySQL connection + executor primitives, layered on `tools-mcp-core`.
```

- [ ] **Step 5: Verify the binary still builds + tests still pass**

Run: `cargo build && cargo test`
Expected: 26 tests pass. The two new lib crates are empty placeholders; `cargo` builds them but they have no code to break.

If the build fails, the most likely culprit is the dep list in `crates/tools-mcp/Cargo.toml` drifting from the original. Diff it against `git show HEAD:Cargo.toml`.

- [ ] **Step 6: Update `Makefile`**

`make` invocations all become workspace-aware automatically (`cargo build` from the root targets the whole workspace). The single-binary targets (`make run ARGS=...`) need to scope to the bin crate:

```makefile
run: ## Run the bin (tools-mcp). Use ARGS=...
	$(CARGO) run -p tools-mcp -- $(ARGS)

install: release
	$(CARGO) install --path crates/tools-mcp --force
```

Update those two targets in `Makefile`. Other targets (`build`, `test`, `clippy`, `fmt`) work on the whole workspace, leave alone.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: bootstrap cargo workspace with empty core/mysql crates

Moves the existing single-crate code into crates/tools-mcp/ and creates
empty crates/tools-mcp-core and crates/tools-mcp-mysql placeholders.
Top-level Cargo.toml becomes a workspace manifest only.

Pure mechanical move; all 26 tests still pass. Subsequent tasks fill
in the new lib crates.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Migrate shared types into `tools-mcp-core`

**Files:**
- Modify: `crates/tools-mcp-core/src/lib.rs` (fill in)
- Modify: `crates/tools-mcp/src/error.rs` (delete; replaced by re-export of `tools_mcp_core::Error`)
- Modify: `crates/tools-mcp/src/tunnel/traits.rs` (delete; bin uses `tools_mcp_core::{Tunnel, TunnelEndpoint}`)
- Modify: `crates/tools-mcp/src/connection/traits.rs` (delete; bin uses `tools_mcp_core::Connection`)
- Modify: `crates/tools-mcp/src/output/types.rs` (delete; bin uses `tools_mcp_core::ExecutionResult`)
- Modify: `crates/tools-mcp/src/lib.rs` (drop now-empty modules; the `error` module becomes a `pub use` of `tools_mcp_core::Error/Result`)
- Modify: `crates/tools-mcp/Cargo.toml` (add `tools-mcp-core = { path = "../tools-mcp-core" }`)
- Modify many call sites that import `crate::error::Error` etc.

- [ ] **Step 1: Fill in `crates/tools-mcp-core/src/lib.rs`**

```rust
//! Core traits and shared types for the tools-mcp workspace.
//!
//! This crate is the dependency floor: only `async-trait` and `serde`.
//! Service-specific code (MySQL, SSH, etc.) lives in higher crates.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

// -- Error --------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    Config(String),
    Connection(String),
    Execution(String),
    Io(std::io::Error),
    /// Errors from a specific service (MySQL, SSH library, YAML parser, …).
    /// Higher crates wrap their library errors into this variant via
    /// `Error::Service(format!("{e}"))` to keep core dep-free.
    Service(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(msg) => write!(f, "Configuration error: {msg}"),
            Error::Connection(msg) => write!(f, "Connection error: {msg}"),
            Error::Execution(msg) => write!(f, "Execution error: {msg}"),
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Service(msg) => write!(f, "Service error: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Config(_) | Error::Connection(_) | Error::Execution(_) | Error::Service(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// -- Tunnel -------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TunnelEndpoint {
    pub host: String,
    pub port: u16,
}

#[async_trait]
pub trait Tunnel: Send + Sync {
    async fn establish(&mut self) -> Result<TunnelEndpoint>;
    async fn close(&mut self) -> Result<()>;
    fn is_active(&self) -> bool;
}

// -- Connection ---------------------------------------------------------

#[async_trait]
pub trait Connection: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
}

// -- ExecutionResult ----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub affected_rows: u64,
}

impl ExecutionResult {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<String>>, affected_rows: u64) -> Self {
        Self { columns, rows, affected_rows }
    }
}
```

- [ ] **Step 2: Add `tools-mcp-core` as a dep of `tools-mcp`**

In `crates/tools-mcp/Cargo.toml` `[dependencies]`, add:

```toml
tools-mcp-core = { path = "../tools-mcp-core" }
```

Also you can DROP these from the bin's `[dependencies]`: nothing yet — `serde_yml` and `toml` are still used by `config`, `mysql_async` is still used by the bin's tunnel/connection wrappers.

- [ ] **Step 3: Delete the now-redundant modules in the bin**

Delete files:
- `crates/tools-mcp/src/error.rs`
- `crates/tools-mcp/src/tunnel/traits.rs`
- `crates/tools-mcp/src/connection/traits.rs`
- `crates/tools-mcp/src/output/types.rs`

Update `crates/tools-mcp/src/lib.rs`:

```rust
pub mod cli;
pub mod config;
pub mod connection;
pub mod core;
pub mod executor;
pub mod mcp;
pub mod output;
pub mod tunnel;

pub use tools_mcp_core::{Error, Result};
```

(The `error` module is gone; we re-export from core. Same `crate::Error` / `crate::Result` short paths still work for downstream code in the bin.)

- [ ] **Step 4: Fix the `tunnel`, `connection`, `output` module entry points**

`crates/tools-mcp/src/tunnel/mod.rs`:

```rust
mod direct;
mod ssh;

pub use direct::DirectTunnel;
pub use ssh::SshTunnel;
pub use tools_mcp_core::{Tunnel, TunnelEndpoint};
```

`crates/tools-mcp/src/connection/mod.rs`:

```rust
mod mysql;

pub use mysql::MySQLConnection;
pub use tools_mcp_core::Connection;
```

`crates/tools-mcp/src/output/mod.rs`:

```rust
mod cli;

pub use cli::CliFormatter;
pub use tools_mcp_core::ExecutionResult;
```

- [ ] **Step 5: Fix call sites**

The error type changes affect a few call sites:

5a. `crates/tools-mcp/src/config/loader.rs` — replace `Error::Yaml(serde_yml::Error)` and `Error::Toml(toml::de::Error)` From-impls with explicit map_err to `Error::Config(format!(...))`. The current loader code already wraps `serde_yml::from_str` errors in `Error::Config(format!("invalid YAML in '{}': {e}", ...))` — that pattern stays. Search the file for `Error::Yaml` or `?` on a serde_yml/toml return; convert to `.map_err(|e| Error::Config(format!(...)))`.

5b. `crates/tools-mcp/src/connection/mysql.rs` — replace `Error::Mysql(mysql_async::Error)` From-impls with explicit map_err to `Error::Service(format!("{e}"))`. There are 3-4 sites: `pool.get_conn().await?`, `pool.disconnect().await?`. Wrap each with `.map_err(|e: mysql_async::Error| Error::Service(format!("MySQL: {e}")))`.

5c. `crates/tools-mcp/src/executor/mysql.rs` — same pattern: `mysql_conn.query(query).await?` becomes `.map_err(|e: mysql_async::Error| Error::Service(format!("MySQL query: {e}")))`.

5d. `crates/tools-mcp/src/tunnel/ssh.rs` — anywhere russh errors propagate: `.map_err(|e| Error::Connection(format!("SSH ...: {e}")))`. The current code already does this, so likely no change.

The compiler is your friend — `cargo build` will list every remaining `Error::Yaml` / `Error::Toml` / `Error::Mysql` reference; just convert each to either `Service` (for service-specific) or `Config` (for config-parsing) variants.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: 26 pass. If any fail, the failing test most likely matches an `Error::Yaml(...)` / `Error::Mysql(...)` / `Error::Toml(...)` pattern that no longer exists — replace those with `matches!(err, Error::Config(_))` or `matches!(err, Error::Service(_))` as appropriate.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(core): move shared types into tools-mcp-core

Tunnel/Connection traits, TunnelEndpoint, ExecutionResult, and the
Error/Result alias now live in crates/tools-mcp-core. The bin re-exports
Error/Result so call sites that say crate::Error keep working.

Service-specific Error variants (Yaml/Toml/Mysql) collapsed into a
generic Service(String); core stays free of serde_yml/toml/mysql_async.
Wrap-points (config loader, MySQL connection/executor) explicitly
build Error::Service(format!(...)) at the boundary.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Migrate MySQL primitives into `tools-mcp-mysql`

**Files:**
- Create: `crates/tools-mcp-mysql/src/{lib.rs,connection.rs,executor.rs,execute.rs}`
- Move from bin: `connection/mysql.rs` and `executor/mysql.rs` content (their tests come along)
- Modify: `crates/tools-mcp-mysql/Cargo.toml` (already has core + mysql_async + async-trait from Task 1)
- Modify: bin's `connection/mod.rs`, `executor/mod.rs` to re-export from `tools-mcp-mysql`
- Modify: bin's `core/mysql.rs` — orchestrator becomes thin

- [ ] **Step 1: Fill in `crates/tools-mcp-mysql/src/connection.rs`**

Copy the entire content of `crates/tools-mcp/src/connection/mysql.rs` (the body, including the `#[cfg(test)]` block), then change imports from `crate::*` to `tools_mcp_core::*`:

```rust
use async_trait::async_trait;
use mysql_async::{Conn, OptsBuilder, Pool};
use tools_mcp_core::{Connection, Error, Result, Tunnel};

// MySQLConnection struct + impl + Connection impl + tests — content
// identical to the old crates/tools-mcp/src/connection/mysql.rs.

pub struct MySQLConnection {
    tunnel: Box<dyn Tunnel>,
    user: String,
    password: Option<String>,
    database: Option<String>,
    pool: Option<Pool>,
    conn: Option<Conn>,
}

impl MySQLConnection {
    pub fn new(
        tunnel: Box<dyn Tunnel>,
        user: String,
        password: Option<String>,
        database: Option<String>,
    ) -> Self { /* unchanged body */ }

    pub async fn get_conn(&mut self) -> Result<&mut Conn> { /* unchanged body */ }
}

#[async_trait]
impl Connection for MySQLConnection {
    async fn connect(&mut self) -> Result<()> { /* unchanged body, but Mysql wrap → Service wrap */ }
    async fn disconnect(&mut self) -> Result<()> { /* unchanged body, but Mysql wrap → Service wrap */ }
    fn is_connected(&self) -> bool { self.conn.is_some() }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The DirectTunnel reference in the existing test moves with the test —
    // but DirectTunnel is in the bin crate, not in core or mysql. Solution:
    // define a tiny TestTunnel inside this test module that impl Tunnel and
    // returns a fixed endpoint. That decouples the unit test from the bin.
    use async_trait::async_trait;
    use tools_mcp_core::TunnelEndpoint;

    struct TestTunnel { active: bool }

    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint { host: "localhost".into(), port: 3306 })
        }
        async fn close(&mut self) -> Result<()> { self.active = false; Ok(()) }
        fn is_active(&self) -> bool { self.active }
    }

    #[tokio::test]
    async fn test_mysql_connection_new() {
        let tunnel = Box::new(TestTunnel { active: false });
        let conn = MySQLConnection::new(
            tunnel,
            "root".to_string(),
            Some("password".to_string()),
            None,
        );
        assert!(!conn.is_connected());
    }
}
```

- [ ] **Step 2: Fill in `crates/tools-mcp-mysql/src/executor.rs`**

Copy the entire content of `crates/tools-mcp/src/executor/mysql.rs`, change imports to `tools_mcp_core::*` and `crate::connection::MySQLConnection`:

```rust
use mysql_async::{prelude::*, Row, Value};
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::connection::MySQLConnection;

pub struct MySQLExecutor;

impl MySQLExecutor {
    pub async fn execute(
        conn: &mut MySQLConnection,
        query: &str,
    ) -> Result<ExecutionResult> { /* unchanged body, mysql_async error wrap → Service */ }

    fn value_to_string(value: &Value) -> String { /* unchanged */ }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_executor_new() {
        let _executor = MySQLExecutor;
    }
}
```

- [ ] **Step 3: Add `crates/tools-mcp-mysql/src/execute.rs` — the entry function**

```rust
//! Top-level entry: build a MySQL connection over the supplied tunnel,
//! run a single query, and return the structured result.

use tools_mcp_core::{ExecutionResult, Result, Tunnel};

use crate::connection::MySQLConnection;
use crate::executor::MySQLExecutor;

/// Required MySQL connection parameters (post-merge in the caller).
#[derive(Debug, Clone)]
pub struct MysqlParams {
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

/// Execute a single MySQL query through `tunnel`. Always tears down the
/// connection (and via Drop, the tunnel) before returning.
pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: MysqlParams,
    query: &str,
) -> Result<ExecutionResult> {
    let mut conn = MySQLConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = MySQLExecutor::execute(&mut conn, query).await;
    let _ = tools_mcp_core::Connection::disconnect(&mut conn).await;
    exec_result
}
```

- [ ] **Step 4: Fill in `crates/tools-mcp-mysql/src/lib.rs`**

```rust
//! MySQL connection + executor primitives, layered on `tools-mcp-core`.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::MySQLConnection;
pub use execute::{execute, MysqlParams};
pub use executor::MySQLExecutor;
```

- [ ] **Step 5: Add `tools-mcp-mysql` as a dep of the bin**

In `crates/tools-mcp/Cargo.toml` `[dependencies]`, add:

```toml
tools-mcp-mysql = { path = "../tools-mcp-mysql" }
```

You can ALSO drop `mysql_async` from the bin's deps now — only the lib needs it. Verify by running `cargo build` after the deletion below; if any `mysql_async::*` reference remains in the bin, restore the dep.

- [ ] **Step 6: Replace the bin's MySQL primitives with re-exports**

Delete files:
- `crates/tools-mcp/src/connection/mysql.rs`
- `crates/tools-mcp/src/executor/mysql.rs`
- `crates/tools-mcp/src/executor/mod.rs` (becomes a thin re-export below)

Update `crates/tools-mcp/src/connection/mod.rs`:

```rust
pub use tools_mcp_core::Connection;
pub use tools_mcp_mysql::MySQLConnection;
```

Update `crates/tools-mcp/src/lib.rs` — remove `pub mod executor;` and replace with a re-export:

```rust
pub mod cli;
pub mod config;
pub mod connection;
pub mod core;
pub mod mcp;
pub mod output;
pub mod tunnel;

pub use tools_mcp_core::{Error, Result};
pub use tools_mcp_mysql::MySQLExecutor;
```

(Or delete `executor` references entirely; they were only used by `core::mysql::execute`.)

- [ ] **Step 7: Replace `crates/tools-mcp/src/core/mysql.rs` with the orchestrator**

Replace its entire body with:

```rust
//! Orchestrator: take a fully-resolved Config, build the right tunnel,
//! call into tools_mcp_mysql::execute. CLI handler and MCP tool both
//! delegate here so teardown semantics are identical.

use tools_mcp_core::{ExecutionResult, Tunnel};
use tools_mcp_mysql::{execute as mysql_execute, MysqlParams};

use crate::config::{Config, TunnelConfig};
use crate::error::{Error, Result};
use crate::tunnel::{DirectTunnel, SshTunnel};

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

    let params = MysqlParams {
        user,
        password: config.password,
        database: config.database,
    };

    mysql_execute(tunnel, params, query).await
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

The bin no longer constructs MySQLConnection / MySQLExecutor directly — that work moved to `tools_mcp_mysql::execute`. The bin's job here is just:
- Validate Config has required fields
- Build the right tunnel (Direct or Ssh) using bin-local impls
- Translate Config into the lib's MysqlParams
- Call into the lib

- [ ] **Step 8: Verify**

Run: `cargo test`
Expected: 26 pass.
Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

If `mysql_async` is referenced anywhere in the bin (search with `grep -r mysql_async crates/tools-mcp/src`), restore it as a bin dep.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor(mysql): move MySQLConnection + MySQLExecutor into tools-mcp-mysql

The bin's core::mysql::execute is now a thin orchestrator that builds
the right tunnel from Config, then calls tools_mcp_mysql::execute(tunnel,
params, query). All MySQL-specific code (mysql_async dep included) lives
in the new lib crate.

The bin re-exports MySQLConnection / MySQLExecutor for backwards
compatibility of crate::connection::MySQLConnection / crate::MySQLExecutor
import paths used in cli/handler.rs and mcp/tools.rs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Final verification

**Files:** none (verification only).

- [ ] **Step 1: Full test suite**

Run: `cargo test`
Expected: 26 pass (Phase 3 baseline).

- [ ] **Step 2: Lint**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Format**

Run: `cargo fmt --all -- --check`
Expected: clean (run `cargo fmt --all` to fix if not).

- [ ] **Step 4: Release build**

Run: `cargo build --release`
Expected: workspace builds; binary at `target/release/tools-mcp`.

- [ ] **Step 5: CLI smoke (regression)**

Run: `./target/release/tools-mcp mysql --help | head -3`
Expected:
```
Execute a MySQL query

Usage: tools-mcp [GLOBAL OPTIONS] mysql [OPTIONS] <QUERY>
```

- [ ] **Step 6: MCP smoke** (already covered by tests/mcp_smoke.rs)

- [ ] **Step 7: Plugin install path still works**

Run: `cargo install --path crates/tools-mcp --force`
Expected: binary updated at `~/.cargo/bin/tools-mcp`. The Makefile already points `make install` at `crates/tools-mcp` after Task 1.

---

## Task 5: Update README + CLAUDE.md + AGENTS.md

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: README — update Installation and Development sections**

Installation section: change `cargo install --path .` to `cargo install --path crates/tools-mcp`. Same for the cp form.

Development section: nothing changes structurally; `cargo test` / `cargo build` from the workspace root still work.

- [ ] **Step 2: CLAUDE.md and AGENTS.md — module map + workspace note**

Both files: replace the existing Module map table with one that reflects the workspace structure. Recommended row set:

```markdown
| Crate / Module | Role |
| --- | --- |
| `tools-mcp-core` (lib) | `Tunnel` / `Connection` async traits, `TunnelEndpoint`, `Error`/`Result`, `ExecutionResult`. Sole external dep: `async-trait`. The dependency floor for the workspace. |
| `tools-mcp-mysql` (lib) | `MySQLConnection` (impl `core::Connection`), `MySQLExecutor`, and the entry `execute(tunnel, params, query) -> ExecutionResult`. Owns the `mysql_async` dep. Service-agnostic about how the tunnel was built. |
| `tools-mcp` bin: `cli::*` | clap `Cli`, `SshTunnelArgs`, `CliHandler` — CLI mode parse + dispatch. |
| `tools-mcp` bin: `mcp::*` | rmcp `ServerHandler`, `mysql_exec` tool wiring, params → `Config` conversion. |
| `tools-mcp` bin: `config::*` | `Config`, `Profile`, `TunnelConfig`, `ConfigLoader`, `ConfigMerger`. Three-layer merge logic. |
| `tools-mcp` bin: `tunnel::{direct,ssh}` | `DirectTunnel` and `SshTunnel` (russh) — the actual `Tunnel` trait impls. Stay in the bin so `tools-mcp-core` stays russh-free. |
| `tools-mcp` bin: `core::mysql::execute(Config, &str)` | Orchestrator: validate Config, build the right tunnel, translate to `tools_mcp_mysql::MysqlParams`, call into the lib. CLI handler and MCP tool both delegate here. |
| `tools-mcp` bin: `output::CliFormatter` | comfy-table renderer for CLI mode. Operates on `tools_mcp_core::ExecutionResult`. |
```

Add a new top-level note in the Architecture section: "**Workspace.** The repo is a Cargo workspace with three crates under `crates/`. `tools-mcp-core` is service-agnostic (traits + shared types), `tools-mcp-mysql` is MySQL-specific (mysql_async), and `tools-mcp` is the binary that wires them up plus the CLI/MCP/config presentation. Adding a new service (Redis, SSH-direct) means a new sibling lib crate (`tools-mcp-redis`, …) plus a tunnel-dependent orchestrator in the bin."

In **Conventions worth knowing**, update the `Error::source()` bullet:

```markdown
- **`Error::source()`**: lives in `tools-mcp-core`. The wrapping variant is `Error::Service(String)` — service-specific error types (mysql_async::Error, russh::Error, serde_yml::Error, toml::de::Error) are flattened to a string at the boundary so core stays dep-free. If a future service has a need to expose typed inner errors through `source()`, prefer adding a typed variant to `tools-mcp-<service>::Error` and only flattening at the bin boundary.
```

- [ ] **Step 3: Verify diff**

`diff <(tail -n +5 CLAUDE.md) <(tail -n +5 AGENTS.md)` — only the cross-link line + pre-existing methodology line should differ.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs: document the Phase 4 workspace split

- README: cargo install path now under crates/tools-mcp
- CLAUDE.md / AGENTS.md: replace single-crate module map with a
  workspace-aware one (core / mysql / bin); add a workspace note;
  update the Error::source() convention for the new Service variant.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Summary

After Phase 4:

- The repo is a Cargo workspace with three crates under `crates/`.
- `tools-mcp-core` is the dependency floor (async-trait + serde, no russh / no mysql_async / no clap).
- `tools-mcp-mysql` provides the MySQL primitives + a thin entry function. Other services (Redis, SSH-direct, …) will each become their own lib crate sibling.
- The bin (`tools-mcp`) keeps clap CLI, rmcp MCP server, Config types + 3-layer merge, DirectTunnel + SshTunnel, CliFormatter, and the orchestrator that converts Config → tunnel + lib params.
- All 26 tests still pass; clippy clean; release build clean.
- `cargo install --path crates/tools-mcp` is the new install command.

**Deferred to Phase 5+:** Redis lib crate, SSH-direct lib crate. Adding either is the same template: new `tools-mcp-<service>` crate + a thin orchestrator in the bin's `core::<service>::execute(Config, &str)`.
