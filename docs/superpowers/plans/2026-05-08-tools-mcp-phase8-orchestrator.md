# Tools MCP Phase 8: Service Trait + Orchestrator Crate Refactor

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define a unified `Service` trait in `tools-mcp-core` whose `execute(req, tunnel)` signature all four service orchestrators implement, and split the bin's `src/core/` + `src/config/` + `src/tunnel/` into a new `tools-mcp-orchestrator` crate so the bin is purely presentation (CLI + MCP + main).

**Architecture:**
- `tools-mcp-core` (light, trait floor) gains the `Service` trait + the `TunnelConfig` enum (the config-shape — its runtime impls stay elsewhere).
- `tools-mcp-orchestrator` (NEW) holds `Config` / `Profile` / `ConfigLoader` / `ConfigMerger`, `DirectTunnel` + `SshTunnel` impls, and the four `MysqlOrchestrator` / `RedisOrchestrator` / `HttpOrchestrator` / `SshDirectOrchestrator` types — each `impl Service`.
- MySQL and Redis orchestrators get typed `MysqlRequest` / `RedisRequest` types instead of taking the loose `Config` (consistent with HTTP/SSH).
- Profile/YAML 3-layer merge stays in bin (CLI handler / MCP tool) — produces typed Request before calling orchestrator.
- The bin shrinks to `cli/`, `mcp/`, `main.rs`.

**Tech Stack:** No new external deps. Pure code reorganization + the `Service` trait + typed requests for mysql/redis.

**Out of scope (Phase 9+):**
- Per-hop SSH auth overrides.
- HTTP / SSH-direct profile/YAML config (deliberately deferred since Phase 6/7).
- Redis cluster / pub-sub / scripting.
- The fourth-time-shared `build_tunnel_config_for_<svc>` helper extraction (could happen in this Phase as a bonus, but the plan keeps duplicates for now).

---

## File Structure (after refactor)

```
tools-mcp/
├── Cargo.toml                       (workspace + bin: cli + mcp + main)
├── src/                             (THIN — only presentation)
│   ├── main.rs
│   ├── lib.rs
│   ├── cli/
│   └── mcp/
└── crates/
    ├── tools-mcp-core/              (UNCHANGED + adds Service trait + TunnelConfig)
    │   └── src/lib.rs
    ├── tools-mcp-mysql/             (UNCHANGED)
    ├── tools-mcp-redis/             (UNCHANGED)
    ├── tools-mcp-http/              (UNCHANGED)
    ├── tools-mcp-ssh/               (UNCHANGED)
    └── tools-mcp-orchestrator/      (NEW)
        └── src/
            ├── lib.rs
            ├── config/
            │   ├── mod.rs
            │   ├── types.rs         (Config, Profile, ServiceType)
            │   ├── loader.rs        (ConfigLoader)
            │   └── merger.rs        (ConfigMerger)
            ├── tunnel/
            │   ├── mod.rs
            │   ├── direct.rs        (DirectTunnel)
            │   └── ssh.rs           (SshTunnel)
            ├── mysql.rs             (MysqlRequest + MysqlOrchestrator)
            ├── redis.rs             (RedisRequest + RedisOrchestrator)
            ├── http.rs              (HttpOrchestrator wraps tools_mcp_http::HttpRequestSpec)
            └── ssh.rs               (SshDirectOrchestrator wraps tools_mcp_ssh::SshExecRequest)
```

What moves where:

- `tools-mcp-core/src/lib.rs` — APPEND `Service` trait + `TunnelConfig` enum (with custom `deserialize_string_or_vec` helper).
- `tools-mcp/src/config/{types,loader,merger}.rs` → `tools-mcp-orchestrator/src/config/...`. Delete `TunnelConfig` definition from there (it's now in core).
- `tools-mcp/src/tunnel/{direct,ssh}.rs` → `tools-mcp-orchestrator/src/tunnel/...`. The `tools-mcp-ssh::session` re-import in `ssh.rs` continues to work.
- `tools-mcp/src/core/{mysql,redis,http,ssh}.rs` → `tools-mcp-orchestrator/src/{mysql,redis,http,ssh}.rs`, but rewritten as `Orchestrator` types implementing `Service`.

Bin simplifies:
- `src/lib.rs` — drops `pub mod config; pub mod core; pub mod tunnel;` (those modules are gone from bin).
- `src/cli/handler.rs` — imports types from `tools_mcp_orchestrator::*`. Builds typed `MysqlRequest` / `RedisRequest` via 3-layer merge BEFORE calling orchestrator.
- `src/mcp/tools.rs` — same: builds typed requests from JSON params, calls orchestrators.

---

## Task 1: `Service` trait + `TunnelConfig` move into `tools-mcp-core`

**Files:**
- Modify: `crates/tools-mcp-core/src/lib.rs` (append `Service` trait + `TunnelConfig` enum + helpers)
- Modify: `tools-mcp/src/config/types.rs` (will reference `TunnelConfig` from core; for now leave the local defn — Task 2 deletes it)

- [ ] **Step 1: Append `TunnelConfig` and the deserialize helper to `tools-mcp-core/src/lib.rs`**

After the existing `ExecutionResult` block, add:

```rust
// -- TunnelConfig -------------------------------------------------------

/// Tunnel selection plus its parameters. Shared shape across all services.
/// Runtime impls (DirectTunnel, SshTunnel) live in `tools-mcp-orchestrator`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TunnelConfig {
    Direct,
    Ssh {
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

fn default_ssh_port() -> u16 {
    22
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

(These are byte-identical to what's currently in `tools-mcp/src/config/types.rs`. They migrate to core — Task 2 will delete the bin's local copy.)

- [ ] **Step 2: Append the `Service` trait to `tools-mcp-core/src/lib.rs`**

After the `TunnelConfig` block, add:

```rust
// -- Service trait ------------------------------------------------------

/// A service orchestrator: takes a typed request + an optional tunnel
/// config, returns a structured result. All four bundled services
/// (MySQL, Redis, HTTP, SSH-direct) implement this in
/// `tools-mcp-orchestrator`. CLI/MCP layers build the typed request
/// (resolving Profile/YAML/CLI args before this point) and dispatch.
#[async_trait]
pub trait Service {
    /// Service-specific request shape. CLI handler / MCP tool builds
    /// this from user input.
    type Request;

    async fn execute(
        req: Self::Request,
        tunnel: Option<TunnelConfig>,
    ) -> Result<ExecutionResult>;
}
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean. `tools-mcp-core` now has `TunnelConfig` and `Service`. The bin still has its own `TunnelConfig` in `src/config/types.rs` — both will coexist briefly until Task 2 removes the bin's copy. **Do not delete the bin's copy yet.** Check that the bin still compiles by referring to `crate::config::TunnelConfig`.

Run: `cargo test`
Expected: prior count passes (no behavior change yet).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core): add Service trait + TunnelConfig in tools-mcp-core

The Service trait unifies the four service-orchestrator signatures
that Phases 4-7 each invented separately:
  async fn execute(Self::Request, Option<TunnelConfig>) -> Result<ExecutionResult>

TunnelConfig moves into core (config-shape only; runtime tunnel impls
remain in the bin until Task 4 / 5).

Phase 8 hasn't actually swapped any orchestrator over to the trait yet
— that's Tasks 5-8. The bin's config/types.rs still defines its own
TunnelConfig; subsequent tasks delete that and re-import from core.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Bootstrap `tools-mcp-orchestrator` crate; move `Config` types

**Files:**
- Create: `crates/tools-mcp-orchestrator/Cargo.toml`
- Create: `crates/tools-mcp-orchestrator/src/lib.rs`
- Create: `crates/tools-mcp-orchestrator/src/config/{mod,types,loader,merger}.rs`
- Modify: `Cargo.toml` (workspace members + bin dep)
- Delete: `tools-mcp/src/config/{types,loader,merger,mod}.rs` (after Step 4 import-rewrite)

- [ ] **Step 1: Create `crates/tools-mcp-orchestrator/Cargo.toml`**

```toml
[package]
name = "tools-mcp-orchestrator"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_yml = "0.0.12"
toml = "0.8"
tools-mcp-core = { path = "../tools-mcp-core" }

[dev-dependencies]
tempfile = "3.12"
```

(russh, mysql_async, redis, reqwest will join in Tasks 3-7 as the orchestrators arrive.)

- [ ] **Step 2: Create `crates/tools-mcp-orchestrator/src/lib.rs`**

```rust
//! Orchestrator layer: glues service libs (mysql/redis/http/ssh) together
//! with the `Service` trait, the `Config` / `Profile` / `ConfigLoader` /
//! `ConfigMerger` types for 3-layer merge, and the `DirectTunnel` /
//! `SshTunnel` runtime impls. The bin (cli + mcp) calls into here.

pub mod config;
```

- [ ] **Step 3: Move config files**

Copy (don't `git mv` — the destination has a slightly different module path):

- `tools-mcp/src/config/types.rs` → `crates/tools-mcp-orchestrator/src/config/types.rs`. **Edit:** delete the local `TunnelConfig` enum + `deserialize_string_or_vec` helper + `default_ssh_port` helper (they're now in core); replace any internal use of those with `tools_mcp_core::TunnelConfig` + `tools_mcp_core::default_ssh_port` (the latter is only needed if Profile uses `default_ssh_port` directly — it doesn't; the helper is private to `TunnelConfig`'s deserialization, so just deleting it is enough).
- `tools-mcp/src/config/loader.rs` → `crates/tools-mcp-orchestrator/src/config/loader.rs`. Adjust `use` imports: `use crate::config::{Config, TomlConfig};` stays, since the new crate's module structure mirrors. `use tools_mcp_core::{Error, Result};` already correct.
- `tools-mcp/src/config/merger.rs` → `crates/tools-mcp-orchestrator/src/config/merger.rs`. Imports: `use crate::config::Config;` stays.
- `tools-mcp/src/config/mod.rs` → `crates/tools-mcp-orchestrator/src/config/mod.rs`. Content (mirror what's in bin currently — likely `pub mod types; pub mod loader; pub mod merger; pub use types::*; pub use loader::*; pub use merger::*;` or similar). Read the original first.

`Profile.tunnel: Option<TunnelConfig>` becomes `Option<tools_mcp_core::TunnelConfig>` after the local deletion. Add the `use tools_mcp_core::TunnelConfig;` at the top of the new `types.rs`.

The `ServiceType` enum stays in the new types.rs.

- [ ] **Step 4: Wire workspace + bin dep**

Update root `Cargo.toml`:

a) Add `crates/tools-mcp-orchestrator` to `[workspace] members` (alphabetical).

b) Add to bin `[dependencies]`:
```toml
tools-mcp-orchestrator = { path = "crates/tools-mcp-orchestrator" }
```

- [ ] **Step 5: Update bin to import config types from the new crate**

In `tools-mcp/src/lib.rs`: REMOVE `pub mod config;`.

Anywhere in the bin that says `crate::config::Config` / `crate::config::Profile` / `crate::config::ConfigLoader` / `crate::config::ConfigMerger` / `crate::config::ServiceType` / `crate::config::TomlConfig`, change to `tools_mcp_orchestrator::config::*` (or add a `use tools_mcp_orchestrator::config::{Config, Profile, ...};` at the top of each affected file).

`crate::config::TunnelConfig` becomes `tools_mcp_core::TunnelConfig` (it's now in core, not the orchestrator).

Files affected (search & replace):
- `tools-mcp/src/cli/handler.rs`
- `tools-mcp/src/mcp/tools.rs`
- `tools-mcp/src/core/{mysql,redis,http,ssh}.rs` (these reference `crate::config::TunnelConfig`)

Use the compiler — `cargo build` will complain about every wrong import; fix in order.

- [ ] **Step 6: Delete the bin's `src/config/` directory**

Run: `rm -rf tools-mcp/src/config`. (Or use `git rm -r src/config` if you prefer the git move audit trail.)

- [ ] **Step 7: Verify**

Run: `cargo build && cargo test`
Expected: clean, all prior tests pass.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(orchestrator): move Config / Profile / Loader / Merger into orchestrator crate

Bin's src/config/* moves to crates/tools-mcp-orchestrator/src/config/.
Local TunnelConfig defn deleted (moved to core in Task 1). Bin imports
config types from tools_mcp_orchestrator::config::* and TunnelConfig
from tools_mcp_core::TunnelConfig.

cargo test still passes; no behavior change. Tunnel runtime impls
(DirectTunnel, SshTunnel) and the four orchestrators move in
subsequent tasks.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Move `DirectTunnel` + `SshTunnel` into orchestrator

**Files:**
- Create: `crates/tools-mcp-orchestrator/src/tunnel/{mod,direct,ssh}.rs`
- Modify: `crates/tools-mcp-orchestrator/Cargo.toml` (add russh + tools-mcp-ssh)
- Modify: `crates/tools-mcp-orchestrator/src/lib.rs` (declare `pub mod tunnel;`)
- Delete: `tools-mcp/src/tunnel/{mod,direct,ssh}.rs`

- [ ] **Step 1: Add russh + tools-mcp-ssh deps to orchestrator**

In `crates/tools-mcp-orchestrator/Cargo.toml` `[dependencies]`:

```toml
russh = "0.46"
tokio = { version = "1.40", features = ["sync", "macros", "io-util", "net", "rt"] }
tools-mcp-ssh = { path = "../tools-mcp-ssh" }
```

(SshTunnel imports session helpers from tools-mcp-ssh::session; tokio features cover Mutex / TcpListener / spawn / copy_bidirectional needs.)

- [ ] **Step 2: Move tunnel files**

Copy `tools-mcp/src/tunnel/{mod,direct,ssh}.rs` to `crates/tools-mcp-orchestrator/src/tunnel/`. Imports adjusted only as needed:
- `tools-mcp-orchestrator/src/tunnel/mod.rs` content: same as the bin's current (declares `mod direct; mod ssh;` + `pub use direct::DirectTunnel; pub use ssh::SshTunnel; pub use tools_mcp_core::{Tunnel, TunnelEndpoint};`).
- `direct.rs`: imports `tools_mcp_core::{Result, Tunnel, TunnelEndpoint}` — already correct.
- `ssh.rs`: imports `tools_mcp_ssh::session::{AcceptAnyHostKey, build_session_chain}` — already correct.

- [ ] **Step 3: Wire `pub mod tunnel;` into `crates/tools-mcp-orchestrator/src/lib.rs`**

```rust
//! Orchestrator layer: glues service libs (mysql/redis/http/ssh) together
//! with the `Service` trait, the `Config` / `Profile` / `ConfigLoader` /
//! `ConfigMerger` types for 3-layer merge, and the `DirectTunnel` /
//! `SshTunnel` runtime impls. The bin (cli + mcp) calls into here.

pub mod config;
pub mod tunnel;
```

- [ ] **Step 4: Update bin imports**

In bin source code, replace every `crate::tunnel::DirectTunnel` / `crate::tunnel::SshTunnel` with `tools_mcp_orchestrator::tunnel::DirectTunnel` / `tools_mcp_orchestrator::tunnel::SshTunnel`. Files affected:

- `tools-mcp/src/core/{mysql,redis,http,ssh}.rs` (these are still in bin until Tasks 5-8 move them)

Also DELETE `pub mod tunnel;` from `tools-mcp/src/lib.rs`.

- [ ] **Step 5: Delete bin's `src/tunnel/` directory**

Run: `rm -rf tools-mcp/src/tunnel`.

- [ ] **Step 6: Drop `russh` from bin's `[dependencies]`**

The bin's `Cargo.toml` no longer needs `russh = "0.46"` directly — it only flowed through `src/tunnel/ssh.rs` which is now gone. Remove the line.

- [ ] **Step 7: Verify**

Run: `cargo build && cargo test`
Expected: 30+ tests still pass. `tools-mcp/src/lib.rs` no longer declares `tunnel` (or `config`); both have moved.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(orchestrator): move DirectTunnel + SshTunnel into orchestrator crate

DirectTunnel and SshTunnel runtime impls now live in
crates/tools-mcp-orchestrator/src/tunnel/ alongside the config types.
Bin imports them via tools_mcp_orchestrator::tunnel::*. The bin's
russh direct dep is dropped (only used by SshTunnel, which is gone).

cargo test still passes; no behavior change.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `MysqlOrchestrator` (typed `MysqlRequest` + `Service` impl)

**Files:**
- Create: `crates/tools-mcp-orchestrator/src/mysql.rs`
- Modify: `crates/tools-mcp-orchestrator/Cargo.toml` (add tools-mcp-mysql)
- Modify: `crates/tools-mcp-orchestrator/src/lib.rs`
- Delete: `tools-mcp/src/core/mysql.rs`
- Modify: `tools-mcp/src/cli/handler.rs::execute_mysql` to build typed request
- Modify: `tools-mcp/src/mcp/tools.rs::mysql_exec` to build typed request

- [ ] **Step 1: Add tools-mcp-mysql dep**

In `crates/tools-mcp-orchestrator/Cargo.toml` `[dependencies]`:
```toml
tools-mcp-mysql = { path = "../tools-mcp-mysql" }
```

- [ ] **Step 2: Create `crates/tools-mcp-orchestrator/src/mysql.rs`**

```rust
//! MySQL orchestrator: typed request → `tools_mcp_mysql::execute` with
//! the right tunnel built from the request's tunnel config.

use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools_mcp_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools_mcp_mysql::{MysqlParams, execute as mysql_execute};

/// Typed MySQL request. Caller (CLI handler / MCP tool) resolves
/// Profile/YAML/CLI args into this struct before dispatching.
#[derive(Debug, Clone)]
pub struct MysqlRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub query: String,
}

pub struct MysqlOrchestrator;

#[async_trait]
impl Service for MysqlOrchestrator {
    type Request = MysqlRequest;

    async fn execute(
        req: MysqlRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        let tunnel: Box<dyn Tunnel> = match tunnel_config {
            None | Some(TunnelConfig::Direct) => {
                Box::new(DirectTunnel::new(req.host.clone(), req.port))
            }
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
                    req.host.clone(),
                    req.port,
                )?)
            }
        };

        let params = MysqlParams {
            user: req.user,
            password: req.password,
            database: req.database,
        };

        mysql_execute(tunnel, params, &req.query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_req() -> MysqlRequest {
        MysqlRequest {
            host: "h".to_string(),
            port: 3306,
            user: "u".to_string(),
            password: None,
            database: None,
            query: "SELECT 1".to_string(),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_compiles_and_dispatches_to_direct_tunnel() {
        // We can't actually connect to a fake host in unit tests, but we
        // can at least verify the Service trait impl compiles by calling
        // it with a guaranteed-fast-failure path. The DirectTunnel call
        // chain will fail at TCP connect, so the error is a Service / Io
        // error from mysql_async; we just confirm the call shape is valid.
        let r = MysqlOrchestrator::execute(empty_req(), None).await;
        assert!(r.is_err(), "fake host should fail");
    }
}
```

The orchestrator's `execute` no longer takes `Config` — it takes `MysqlRequest` (already-merged values). The bin's CLI handler / MCP tool now does the Profile/YAML merge and produces `MysqlRequest`.

- [ ] **Step 3: Wire into `crates/tools-mcp-orchestrator/src/lib.rs`**

```rust
pub mod config;
pub mod mysql;
pub mod tunnel;

pub use mysql::{MysqlOrchestrator, MysqlRequest};
```

- [ ] **Step 4: Refactor the bin's `execute_mysql` CLI handler**

In `tools-mcp/src/cli/handler.rs`:

a) Remove the import `crate::core::mysql`. Add `use tools_mcp_orchestrator::{MysqlOrchestrator, MysqlRequest};` and `use tools_mcp_core::Service;`.

b) Update `execute_mysql` to:
1. Run the existing 3-layer merge → produces a `Config` with `host`/`port`/`user`/`password`/`database` resolved.
2. Pull the `tunnel` field off the merged Config to use as `tunnel_config`.
3. Build a `MysqlRequest` from the merged Config + the user's `query` arg.
4. Call `MysqlOrchestrator::execute(req, tunnel_config).await`.
5. Format with `CliFormatter` + `println!`.

Concrete diff (read the existing function body, then apply the swap). The existing function ends with `let result = crate::core::mysql::execute(config, query).await?;`. Replace that block with:

```rust
let host = config.host.ok_or_else(|| {
    Error::Config("MySQL host is required".to_string())
})?;
let port = config.port.unwrap_or(3306);
let user = config.user.ok_or_else(|| {
    Error::Config("MySQL user is required".to_string())
})?;

let req = MysqlRequest {
    host,
    port,
    user,
    password: config.password,
    database: config.database,
    query: query.to_string(),
};

let result = MysqlOrchestrator::execute(req, config.tunnel).await?;
let output = CliFormatter::format(&result);
println!("{output}");
Ok(())
```

(The `Error::Config("MySQL host is required")` validation moves OUT of the orchestrator INTO the handler. That's correct — orchestrator now takes a `host: String` (non-optional), so missing-host validation is the caller's job.)

- [ ] **Step 5: Refactor the bin's `mysql_exec` MCP tool**

In `tools-mcp/src/mcp/tools.rs`:

The existing `mysql_exec` function calls `params_to_config` + `crate::core::mysql::execute`. Replace with the same shape:

```rust
pub async fn mysql_exec(params: MysqlExecParams) -> Result<ExecutionResult> {
    let query = params.query.clone();
    let config = params_to_config(&params)?;

    let host = config.host.ok_or_else(|| {
        Error::Config("MySQL host is required".to_string())
    })?;
    let port = config.port.unwrap_or(3306);
    let user = config.user.ok_or_else(|| {
        Error::Config("MySQL user is required".to_string())
    })?;

    let req = tools_mcp_orchestrator::MysqlRequest {
        host,
        port,
        user,
        password: config.password,
        database: config.database,
        query,
    };

    use tools_mcp_core::Service;
    tools_mcp_orchestrator::MysqlOrchestrator::execute(req, config.tunnel).await
}
```

- [ ] **Step 6: Delete `tools-mcp/src/core/mysql.rs`**

Run: `rm tools-mcp/src/core/mysql.rs`.

Update `tools-mcp/src/core/mod.rs`: remove `pub mod mysql;` (if other core modules remain — they will be moved in Tasks 5-7 — leave `pub mod redis; pub mod http; pub mod ssh;`).

- [ ] **Step 7: Verify**

Run: `cargo build && cargo test`
Expected: 30+ tests pass. The orchestrator's compile-only test ("fake host should fail") may take a few seconds.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(mysql): MysqlOrchestrator impl Service via typed MysqlRequest

The bin's core::mysql::execute(Config, &str) is gone. In its place:
crates/tools-mcp-orchestrator/src/mysql.rs defines
struct MysqlOrchestrator; impl Service for MysqlOrchestrator
{ type Request = MysqlRequest; async fn execute(req, tunnel) -> ... }

Profile/YAML 3-layer merge stays in bin (cli/handler.rs +
mcp/tools.rs). The handler builds MysqlRequest by validating + draining
the merged Config, then calls MysqlOrchestrator::execute.

Architecturally: orchestrator only knows about typed requests (no
Config). All merge logic + missing-field validation lives in the
presentation layer.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: `RedisOrchestrator` (typed `RedisRequest` + `Service` impl)

**Files:**
- Create: `crates/tools-mcp-orchestrator/src/redis.rs`
- Modify: `crates/tools-mcp-orchestrator/Cargo.toml` (add tools-mcp-redis)
- Modify: `crates/tools-mcp-orchestrator/src/lib.rs`
- Delete: `tools-mcp/src/core/redis.rs`
- Modify: `tools-mcp/src/cli/handler.rs::execute_redis` to build typed request
- Modify: `tools-mcp/src/mcp/tools.rs::redis_exec` to build typed request

Mirrors Task 4 exactly — different service. Implementation file:

```rust
//! Redis orchestrator: typed request → `tools_mcp_redis::execute`.

use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools_mcp_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools_mcp_redis::{RedisParams, execute as redis_execute};

#[derive(Debug, Clone)]
pub struct RedisRequest {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub db: u32,
    pub command: String,
}

pub struct RedisOrchestrator;

#[async_trait]
impl Service for RedisOrchestrator {
    type Request = RedisRequest;

    async fn execute(
        req: RedisRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        let tunnel: Box<dyn Tunnel> = match tunnel_config {
            None | Some(TunnelConfig::Direct) => {
                Box::new(DirectTunnel::new(req.host.clone(), req.port))
            }
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
                    req.host.clone(),
                    req.port,
                )?)
            }
        };

        let params = RedisParams {
            password: req.password,
            db: req.db,
        };

        redis_execute(tunnel, params, &req.command).await
    }
}
```

CLI handler `execute_redis` validates `config.host` (Redis target host), defaults port=6379 + db=0, builds RedisRequest. MCP `redis_exec` does the same with JSON params.

Same Cargo.toml dep add (`tools-mcp-redis`), same `pub use` add to orchestrator lib.rs, same delete of `tools-mcp/src/core/redis.rs`, same commit pattern.

(Pattern is mechanical — see Task 4 step structure to apply.)

---

## Task 6: `HttpOrchestrator` (impl `Service`, typed already)

**Files:**
- Create: `crates/tools-mcp-orchestrator/src/http.rs`
- Modify: `crates/tools-mcp-orchestrator/Cargo.toml` (add tools-mcp-http + reqwest for URL parsing)
- Modify: `crates/tools-mcp-orchestrator/src/lib.rs`
- Delete: `tools-mcp/src/core/http.rs`
- Modify: bin handler/tool to call `HttpOrchestrator::execute`

HTTP's existing orchestrator already takes a typed `HttpRequestSpec` + `Option<TunnelConfig>` — just wrap it as a `Service` impl. The Request type is `tools_mcp_http::HttpRequestSpec` directly (no need for an HttpRequest in orchestrator):

```rust
//! HTTP orchestrator: parse URL, build tunnel, dispatch to tools_mcp_http.

use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools_mcp_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools_mcp_http::{HttpRequestSpec, execute as http_execute};

pub struct HttpOrchestrator;

#[async_trait]
impl Service for HttpOrchestrator {
    type Request = HttpRequestSpec;

    async fn execute(
        req: HttpRequestSpec,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        let parsed = reqwest::Url::parse(&req.url)
            .map_err(|e| Error::Config(format!("invalid URL '{}': {e}", req.url)))?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(Error::Config(format!(
                "URL '{}' uses an unsupported scheme '{scheme}' (need http/https)",
                req.url
            )));
        }
        let url_host = parsed
            .host_str()
            .ok_or_else(|| Error::Config(format!("URL '{}' has no host", req.url)))?
            .to_string();
        let url_port = parsed.port_or_known_default().ok_or_else(|| {
            Error::Config(format!(
                "URL '{}' has no port and the scheme provides no default",
                req.url
            ))
        })?;

        let tunnel: Box<dyn Tunnel> = match tunnel_config {
            None | Some(TunnelConfig::Direct) => {
                Box::new(DirectTunnel::new(url_host.clone(), url_port))
            }
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
                    url_host.clone(),
                    url_port,
                )?)
            }
        };

        http_execute(tunnel, url_host, url_port, req).await
    }
}
```

Cargo.toml: add `tools-mcp-http` + `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }` (the orchestrator now does URL parsing, so it needs reqwest::Url).

The bin's existing reqwest dep can be removed once `core::http::execute` is gone (URL parsing migrates with the orchestrator).

---

## Task 7: `SshDirectOrchestrator` (impl `Service`, typed already)

Mirrors Task 6 — SSH-direct already takes `SshExecRequest`. Move the existing `core::ssh::execute` body into a `SshDirectOrchestrator` that impls `Service`. Add `tools-mcp-ssh` to orchestrator's Cargo.toml deps (shared with the Tunnel-side import added in Task 3 — already there).

(Pattern follows Tasks 4-6.)

---

## Task 8: Final verification + drop bin's empty `core::*` references

**Files:**
- Modify: `tools-mcp/src/lib.rs` (drop `pub mod core;` — it's empty now)
- Delete: `tools-mcp/src/core/` if empty
- Modify: `tools-mcp/Cargo.toml` (drop reqwest if no longer used)

- [ ] **Step 1: Drop the empty `core` module from bin**

After Tasks 4-7, `tools-mcp/src/core/{mysql,redis,http,ssh}.rs` are all gone, and `mod.rs` should also be empty. Delete the directory: `rm -rf tools-mcp/src/core`. Drop `pub mod core;` from `tools-mcp/src/lib.rs`.

- [ ] **Step 2: Drop reqwest from bin if unused**

Bin no longer parses URLs (HttpOrchestrator does). Check with `grep reqwest tools-mcp/src/`. If only used by `cargo metadata`, drop the line from bin's `[dependencies]`.

- [ ] **Step 3: Final verification**

Run: `cargo test`
Expected: all tests pass.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean.

Run: `cargo build --release`
Expected: workspace builds.

CLI smoke for all 4 services:
```bash
for s in mysql redis http ssh; do
  ./target/release/tools-mcp $s --help | head -3
done
```
Expected: all 4 still print their respective headers.

MCP smoke (already covered by `tests/mcp_smoke.rs`).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: drop bin's empty core/ + clean up reqwest dep

After Tasks 4-7 moved the four orchestrators into tools-mcp-orchestrator,
the bin's src/core/ is empty — delete the directory + drop the module
declaration. Bin no longer parses URLs (orchestrator does), so drop
reqwest from bin's [dependencies].

Bin is now thin: cli/, mcp/, main.rs. Everything else lives in lib
crates.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Documentation update

**Files:**
- Modify: `README.md` (Architecture section if it mentions the layout)
- Modify: `CLAUDE.md` and `AGENTS.md`

Update the Module map and Phase boundaries to reflect:
- `tools-mcp-core` now hosts `Service` trait + `TunnelConfig`
- `tools-mcp-orchestrator` (new row) hosts Config / Profile / Loader / Merger / DirectTunnel / SshTunnel / 4 orchestrators
- Bin's `src/core/*` is gone — replace those rows with a single "bin: cli + mcp + main" row
- New convention: "every service exposes typed Request via Service trait; CLI/MCP build the Request after merge; orchestrator never sees raw Config"

Sync CLAUDE.md and AGENTS.md (one source of truth pattern). Verify diff is minimal (cross-link + methodology trailer only).

(Concrete edits follow the Phase 4-7 docs-update pattern. See those tasks for the exact format.)

---

## Summary

After Phase 8:

- **`Service` trait** in `tools-mcp-core` unifies the four orchestrators' signature: `async fn execute(Self::Request, Option<TunnelConfig>) -> Result<ExecutionResult>`.
- **`tools-mcp-orchestrator`** crate (NEW) owns: Config types + ConfigLoader/Merger + DirectTunnel/SshTunnel impls + 4 `<svc>Orchestrator` types each impl-ing `Service`.
- **MysqlRequest / RedisRequest** are new typed structs (host/port/user/password/database/query etc.) replacing the loose `(Config, &str)` shape on the orchestrator side. Profile/YAML merge stays in bin's CLI/MCP layer; bin produces typed Request before calling orchestrator.
- **Bin** shrinks to `cli/`, `mcp/`, `main.rs` — pure presentation. No `config/`, no `core/`, no `tunnel/`.
- **Service libs (mysql/redis/http/ssh)** unchanged.
- **CLI ↔ MCP parity**: every CLI subcommand has a paired MCP tool, and both build the same typed Request and call the same `<svc>Orchestrator::execute`. The orchestrator name is the only level where dispatch happens.

**Deferred:**
- Per-service Profile/YAML for HTTP / SSH-direct (still requires fields not yet on `Profile`).
- Cleanup of the four `build_tunnel_config_for_<svc>` MCP-side helpers.
- Per-hop SSH auth.
- Redis cluster routing / pub-sub.
