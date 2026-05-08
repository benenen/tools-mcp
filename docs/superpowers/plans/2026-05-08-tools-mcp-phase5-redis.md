# Tools MCP Phase 5: Redis Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Redis support following the Phase 4 architecture: a new `tools-mcp-redis` lib crate with `RedisConnection`/`RedisExecutor`/`execute(tunnel, params, command_str)`, a thin `core::redis::execute` orchestrator in the bin, a `redis "<COMMAND>"` CLI subcommand, and a `redis_exec` MCP tool that delegates to the same core. Same SSH-tunnel + profile + YAML-config pipeline as MySQL.

**Architecture:** Mirrors Phase 4 / Phase 3:
- `tools-mcp-redis` (new lib) owns `redis = "1.2"` (sole external dep beyond `tools-mcp-core`) and `shlex = "1.3"` for splitting the user-supplied command string into Redis CLI tokens.
- Bin's `core::redis::execute(Config, &str)` is the symmetric orchestrator to `core::mysql::execute`: validate Config, build the right tunnel, translate to `tools_mcp_redis::RedisParams`, call into the lib.
- CLI handler / MCP `redis_exec` tool both delegate to the orchestrator (CLI/MCP parity).
- `Config` and `Profile` gain a `db: Option<u32>` field for Redis's database number (`db: 0` in YAML; `--db 0` on CLI; ignored by MySQL paths).
- Output mapping (Phase 5 simple-mapping): `redis::Value::Nil`, `Int`, `BulkString`, `SimpleString`, `Okay`, and `Array` get specialized columns/rows shapes; everything else (`Map`/`Set`/`Push`/`Attribute`/…) is rendered via `format!("{:?}", value)` into a single-cell row. Sufficient for `GET`/`SET`/`HGETALL`/`LRANGE`/`KEYS` workflows.

**Tech Stack:** [`redis`](https://crates.io/crates/redis) 1.2 (de-facto Rust Redis client) + [`shlex`](https://crates.io/crates/shlex) 1.3 (POSIX shell-style splitting). Reuses existing `tools-mcp-core` (traits, `ExecutionResult`, `Error`/`Result`) and the bin's `DirectTunnel`/`SshTunnel`.

**Out of scope (Phase 6+):**
- Pub/Sub, transactions (MULTI/EXEC), scripting (EVAL).
- Cluster routing.
- Pipelining / batch commands per call.
- Per-Value typed mapping for `Map`/`Set`/`Push` (RESP3 features).

---

## File Structure

**New:**
- `crates/tools-mcp-redis/Cargo.toml` — `redis = "1.2"` (with `tokio-comp` feature) + `shlex = "1.3"` + `tools-mcp-core` + `async-trait`.
- `crates/tools-mcp-redis/src/lib.rs` — re-exports `RedisConnection`, `RedisExecutor`, `execute`, `RedisParams`.
- `crates/tools-mcp-redis/src/connection.rs` — `RedisConnection` (impl `core::Connection`).
- `crates/tools-mcp-redis/src/executor.rs` — `RedisExecutor::run(conn, command_str) -> ExecutionResult`. Owns the shlex parse + `redis::Value` → `ExecutionResult` mapping.
- `crates/tools-mcp-redis/src/execute.rs` — `execute(tunnel, params, command_str)` entry function.

**Modified:**
- `Cargo.toml` (workspace) — add `crates/tools-mcp-redis` to `members`; add `tools-mcp-redis = { path = "crates/tools-mcp-redis" }` to bin `[dependencies]`.
- `src/config/types.rs` — add `pub db: Option<u32>` to `Profile` and `Config`. ConfigMerger / loader pick it up automatically.
- `src/config/merger.rs` — add the `db` field to the per-field `or` chain.
- `src/core/mod.rs` — `pub mod redis;`.
- `src/core/redis.rs` — orchestrator `execute(Config, &str)`.
- `src/cli/args.rs` — add `Commands::Redis { command, host, port, password, db, profile }`.
- `src/cli/handler.rs` — handle the new variant; add an `execute_redis(command, config)` wrapper.
- `src/mcp/tools.rs` — add `RedisExecParams` + `redis_exec(params)` entry function.
- `src/mcp/server.rs` — register the new `redis_exec` tool on `ToolsMcpServer`.
- `commands/redis.md` — new slash command.
- `skills/redis-using/SKILL.md` — new skill.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — document Phase 5.

---

## Task 1: Add `Config.db` / `Profile.db` field + bootstrap empty `tools-mcp-redis` crate

**Files:**
- Modify: `src/config/types.rs` (add `db: Option<u32>` to both `Config` and `Profile`)
- Modify: `src/config/merger.rs` (carry `db` in the `or` chain)
- Modify: `Cargo.toml` (workspace `members` + bin `[dependencies]`)
- Create: `crates/tools-mcp-redis/Cargo.toml`
- Create: `crates/tools-mcp-redis/src/lib.rs` (placeholder)

- [ ] **Step 1: Add `db: Option<u32>` to `Profile` and `Config`**

In `src/config/types.rs`:

a) On the `Profile` struct (the existing field list ends with `tunnel: Option<TunnelConfig>`), add a new field:

```rust
    /// Redis database number. Ignored by non-Redis services.
    pub db: Option<u32>,
```

(Place it AFTER `database` and BEFORE `key_path` so the order stays roughly alphabetical / by logical grouping. Don't move other fields.)

b) On the `Config` struct, do the same — add `pub db: Option<u32>` AFTER `database` and BEFORE `key_path`.

c) On the `ConfigHelper` struct inside the manual `Deserialize for Config` impl (if it still exists — it was deleted in Phase 1 cleanup; confirm by checking the file). If `Config` derives `Deserialize` directly via `#[derive(Deserialize)]` with `#[serde(rename = "type")]` on `service_type`, no helper to update. If a `ConfigHelper` is present, mirror the new `db` field.

- [ ] **Step 2: Carry `db` through `ConfigMerger`**

In `src/config/merger.rs`'s `merge` function, the `or` chain needs the new field. Append:

```rust
            db: override_cfg.db.or(base.db),
```

(Add it after `database: override_cfg.database.or(base.database),` and before `key_path: ...`.)

- [ ] **Step 3: Add the empty `tools-mcp-redis` crate**

Create `crates/tools-mcp-redis/Cargo.toml`:

```toml
[package]
name = "tools-mcp-redis"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
redis = { version = "1.2", features = ["tokio-comp"] }
shlex = "1.3"
tools-mcp-core = { path = "../tools-mcp-core" }

[dev-dependencies]
tokio = { version = "1.40", features = ["macros", "rt-multi-thread"] }
```

(`tokio-comp` enables redis-rs's tokio integration; without it, `get_multiplexed_async_connection` and friends won't compile.)

Create `crates/tools-mcp-redis/src/lib.rs`:

```rust
//! Redis connection + executor primitives, layered on `tools-mcp-core`.
```

- [ ] **Step 4: Wire workspace members and bin dep**

In the root `Cargo.toml`:

a) Update `[workspace] members`:

```toml
[workspace]
resolver = "3"
members = [
    "crates/tools-mcp-core",
    "crates/tools-mcp-mysql",
    "crates/tools-mcp-redis",
]
```

b) Update bin `[dependencies]` — add (alphabetical position between `tools-mcp-mysql` and `toml`):

```toml
tools-mcp-redis = { path = "crates/tools-mcp-redis" }
```

- [ ] **Step 5: Verify**

Run: `cargo build`
Expected: clean. The new crate has no code yet so it compiles trivially. The `db` field doesn't break any existing serde deserialization because it's `Option<u32>`.

Run: `cargo test`
Expected: 26 pass. The schema additions don't affect any existing test.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(config,redis): add Config.db field and tools-mcp-redis stub

- Profile and Config gain db: Option<u32> for Redis database number;
  ignored by non-Redis services. ConfigMerger carries it through.
- New empty crate crates/tools-mcp-redis with redis 1.2 + shlex 1.3
  + tools-mcp-core deps. lib.rs is just a doc comment for now.
- Workspace members updated.

cargo test still 26 pass; no behavior change for the MySQL path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Implement `RedisConnection` (impl `core::Connection`)

**Files:**
- Create: `crates/tools-mcp-redis/src/connection.rs`
- Modify: `crates/tools-mcp-redis/src/lib.rs`

- [ ] **Step 1: Write the connection struct + Connection trait impl**

Create `crates/tools-mcp-redis/src/connection.rs`:

```rust
use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use redis::Client;
use tools_mcp_core::{Connection, Error, Result, Tunnel};

pub struct RedisConnection {
    tunnel: Box<dyn Tunnel>,
    password: Option<String>,
    db: u32,
    client: Option<Client>,
    conn: Option<MultiplexedConnection>,
}

impl RedisConnection {
    pub fn new(tunnel: Box<dyn Tunnel>, password: Option<String>, db: u32) -> Self {
        Self {
            tunnel,
            password,
            db,
            client: None,
            conn: None,
        }
    }

    pub async fn get_conn(&mut self) -> Result<&mut MultiplexedConnection> {
        if self.conn.is_none() {
            self.connect().await?;
        }
        self.conn.as_mut().ok_or_else(|| {
            Error::Connection("Redis connection not established".to_string())
        })
    }
}

#[async_trait]
impl Connection for RedisConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;
        // redis::Client::open accepts a URL string. Build a redis://[:pwd@]host:port/db URL.
        let auth = match &self.password {
            Some(pwd) => format!(":{}@", urlencoding::encode(pwd)),
            None => String::new(),
        };
        let url = format!(
            "redis://{auth}{host}:{port}/{db}",
            host = endpoint.host,
            port = endpoint.port,
            db = self.db,
        );
        let client = Client::open(url)
            .map_err(|e: redis::RedisError| Error::Service(format!("Redis: {e}")))?;
        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e: redis::RedisError| Error::Service(format!("Redis: {e}")))?;
        self.client = Some(client);
        self.conn = Some(conn);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // MultiplexedConnection has no explicit close; dropping it terminates the multiplex.
        self.conn = None;
        self.client = None;
        self.tunnel.close().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.conn.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools_mcp_core::TunnelEndpoint;

    /// Minimal Tunnel impl so this lib's tests don't depend on
    /// DirectTunnel (which lives in the bin crate).
    struct TestTunnel { active: bool }

    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint { host: "localhost".to_string(), port: 6379 })
        }
        async fn close(&mut self) -> Result<()> {
            self.active = false;
            Ok(())
        }
        fn is_active(&self) -> bool { self.active }
    }

    #[tokio::test]
    async fn test_redis_connection_new() {
        let tunnel = Box::new(TestTunnel { active: false });
        let conn = RedisConnection::new(tunnel, Some("password".to_string()), 0);
        assert!(!conn.is_connected());
    }
}
```

Note: this introduces a `urlencoding` dep for safely embedding the password into the URL. Add to `crates/tools-mcp-redis/Cargo.toml` `[dependencies]`:

```toml
urlencoding = "2"
```

(`urlencoding` is a tiny zero-dep crate. Without it, a password containing `@` or `:` would corrupt the URL.)

- [ ] **Step 2: Update `crates/tools-mcp-redis/src/lib.rs`**

```rust
//! Redis connection + executor primitives, layered on `tools-mcp-core`.

pub mod connection;

pub use connection::RedisConnection;
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo test test_redis_connection_new`
Expected: 1 PASS. (The new test runs without a real Redis — it just constructs the connection and checks `is_connected() == false`.)

Run: `cargo test`
Expected: 27 pass (26 prior + 1 new).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(redis): RedisConnection impl tools-mcp-core::Connection

Builds a redis://[:pwd@]host:port/db URL from the tunnel endpoint and
the supplied password/db, then opens a multiplexed async connection.
disconnect() drops the multiplex (which terminates it) and closes the
underlying tunnel.

Adds a urlencoding dep so passwords with special characters don't
corrupt the URL.

Test uses a local TestTunnel — the lib doesn't depend on DirectTunnel
(which lives in the bin).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Implement `RedisExecutor` + `execute()` entry

**Files:**
- Create: `crates/tools-mcp-redis/src/executor.rs`
- Create: `crates/tools-mcp-redis/src/execute.rs`
- Modify: `crates/tools-mcp-redis/src/lib.rs`

- [ ] **Step 1: Write `RedisExecutor` + Value mapping**

Create `crates/tools-mcp-redis/src/executor.rs`:

```rust
use redis::{cmd, Value};
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::connection::RedisConnection;

pub struct RedisExecutor;

impl RedisExecutor {
    /// Run `command_str` (e.g. `"GET foo"` or `"HSET h f1 v1 f2 v2"`) against
    /// `conn` and return the result mapped into an `ExecutionResult`.
    pub async fn run(conn: &mut RedisConnection, command_str: &str) -> Result<ExecutionResult> {
        let tokens = shlex::split(command_str).ok_or_else(|| {
            Error::Execution(format!(
                "failed to parse Redis command (unbalanced quotes?): {command_str}"
            ))
        })?;
        let (cmd_name, args) = tokens
            .split_first()
            .ok_or_else(|| Error::Execution("empty Redis command".to_string()))?;

        let mysql_conn = conn.get_conn().await?;
        let mut redis_cmd = cmd(cmd_name);
        for arg in args {
            redis_cmd.arg(arg);
        }

        let value: Value = redis_cmd
            .query_async(mysql_conn)
            .await
            .map_err(|e: redis::RedisError| Error::Service(format!("Redis: {e}")))?;

        Ok(value_to_result(value))
    }
}

/// Phase 5 simple-mapping: specialize the common variants; debug-format the rest.
fn value_to_result(value: Value) -> ExecutionResult {
    match value {
        Value::Nil => ExecutionResult::new(vec!["result".to_string()], vec![], 0),
        Value::Int(i) => single_cell(i.to_string()),
        Value::BulkString(b) => single_cell(String::from_utf8_lossy(&b).to_string()),
        Value::SimpleString(s) => single_cell(s),
        Value::Okay => single_cell("OK".to_string()),
        Value::Array(items) => {
            let rows: Vec<Vec<String>> = items.into_iter().map(value_to_cell_row).collect();
            let affected = rows.len() as u64;
            ExecutionResult::new(vec!["result".to_string()], rows, affected)
        }
        other => single_cell(format!("{other:?}")),
    }
}

fn single_cell(text: String) -> ExecutionResult {
    ExecutionResult::new(vec!["result".to_string()], vec![vec![text]], 1)
}

/// Recursive helper for Array elements: each element becomes one cell of one row.
/// For nested arrays, we render the nested element as Debug to keep the result
/// rectangular (one column).
fn value_to_cell_row(v: Value) -> Vec<String> {
    let cell = match v {
        Value::Nil => "nil".to_string(),
        Value::Int(i) => i.to_string(),
        Value::BulkString(b) => String::from_utf8_lossy(&b).to_string(),
        Value::SimpleString(s) => s,
        Value::Okay => "OK".to_string(),
        other => format!("{other:?}"),
    };
    vec![cell]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_nil_maps_to_empty_rows() {
        let r = value_to_result(Value::Nil);
        assert_eq!(r.columns, vec!["result".to_string()]);
        assert!(r.rows.is_empty());
        assert_eq!(r.affected_rows, 0);
    }

    #[test]
    fn test_value_int_maps_to_single_cell() {
        let r = value_to_result(Value::Int(42));
        assert_eq!(r.rows, vec![vec!["42".to_string()]]);
        assert_eq!(r.affected_rows, 1);
    }

    #[test]
    fn test_value_bulk_string_maps_to_single_cell() {
        let r = value_to_result(Value::BulkString(b"hello".to_vec()));
        assert_eq!(r.rows, vec![vec!["hello".to_string()]]);
        assert_eq!(r.affected_rows, 1);
    }

    #[test]
    fn test_value_okay_maps_to_ok() {
        let r = value_to_result(Value::Okay);
        assert_eq!(r.rows, vec![vec!["OK".to_string()]]);
    }

    #[test]
    fn test_value_array_maps_to_one_row_per_item() {
        let r = value_to_result(Value::Array(vec![
            Value::BulkString(b"foo".to_vec()),
            Value::BulkString(b"bar".to_vec()),
            Value::Int(7),
        ]));
        assert_eq!(
            r.rows,
            vec![
                vec!["foo".to_string()],
                vec!["bar".to_string()],
                vec!["7".to_string()],
            ]
        );
        assert_eq!(r.affected_rows, 3);
    }
}
```

Notes on the `redis` 1.2 API:
- `redis::cmd(name)` builds a command. `.arg(...)` adds args. `.query_async(conn)` runs.
- `Value` is the response enum. Variants in 1.2 include `Nil`, `Int(i64)`, `BulkString(Vec<u8>)`, `SimpleString(String)`, `Okay`, `Array(Vec<Value>)`, `Map(Vec<(Value, Value)>)`, `Set(Vec<Value>)`, `Double(f64)`, `Boolean(bool)`, `BigNumber`, `VerbatimString { format, text }`, `Push { kind, data }`, `ServerError(_)`, `Attribute { ... }`. We specialize the first 6 plus a recursive Array helper; the rest fall through to `format!("{other:?}")`.
- If your installed redis crate version differs slightly (e.g. `Value::Data` instead of `Value::BulkString`), adapt the match arms — the variant names changed in the 0.27 → 1.0 release. Check `~/.cargo/registry/src/.../redis-1.2.*/src/types/mod.rs` for the actual enum.

- [ ] **Step 2: Add `execute(tunnel, params, command_str)` entry**

Create `crates/tools-mcp-redis/src/execute.rs`:

```rust
//! Top-level entry: build a Redis connection over the supplied tunnel,
//! run a single command, and return the structured result.

use tools_mcp_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::RedisConnection;
use crate::executor::RedisExecutor;

#[derive(Debug, Clone)]
pub struct RedisParams {
    pub password: Option<String>,
    /// Redis database number (0..15 in default Redis configs).
    pub db: u32,
}

/// Execute a single Redis command through `tunnel`. Always tears down the
/// connection (and via Drop, the tunnel) before returning.
pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: RedisParams,
    command_str: &str,
) -> Result<ExecutionResult> {
    let mut conn = RedisConnection::new(tunnel, params.password, params.db);
    let exec_result = RedisExecutor::run(&mut conn, command_str).await;
    let _ = conn.disconnect().await;
    exec_result
}
```

- [ ] **Step 3: Update `crates/tools-mcp-redis/src/lib.rs`**

```rust
//! Redis connection + executor primitives, layered on `tools-mcp-core`.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::RedisConnection;
pub use execute::{RedisParams, execute};
pub use executor::RedisExecutor;
```

- [ ] **Step 4: Verify**

Run: `cargo test --package tools-mcp-redis`
Expected: 6 PASS (5 value-mapping tests + 1 connection-new test).

Run: `cargo test`
Expected: 32 pass (26 prior + 6 new).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(redis): RedisExecutor with shlex parsing + Value mapping

RedisExecutor::run parses the command string via shlex (so quoted args
work), builds a redis::cmd(...).arg(...).arg(...) chain, and runs it
via query_async. The redis::Value response is mapped into an
ExecutionResult with simple specialization for Nil/Int/BulkString/
SimpleString/Okay/Array; less common variants (Map/Set/Push/etc.)
fall through to Debug-format.

execute(tunnel, params, command_str) is the top-level entry function;
the bin's core::redis::execute orchestrator (next task) will call this.

5 unit tests cover the value-mapping branches.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Bin orchestrator `core::redis::execute(Config, &str)`

**Files:**
- Create: `src/core/redis.rs`
- Modify: `src/core/mod.rs`

- [ ] **Step 1: Write the orchestrator**

Create `src/core/redis.rs`:

```rust
//! Orchestrator: take a fully-resolved Config, build the right tunnel,
//! call into tools_mcp_redis::execute. CLI handler and MCP tool both
//! delegate here so teardown semantics are identical.

use crate::config::{Config, TunnelConfig};
use crate::tunnel::{DirectTunnel, SshTunnel};
use tools_mcp_core::{Error, ExecutionResult, Result, Tunnel};
use tools_mcp_redis::{RedisParams, execute as redis_execute};

pub async fn execute(config: Config, command: &str) -> Result<ExecutionResult> {
    let host = config
        .host
        .ok_or_else(|| Error::Config("Redis host is required".to_string()))?;
    let port = config.port.unwrap_or(6379);
    let db = config.db.unwrap_or(0);

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

    let params = RedisParams {
        password: config.password,
        db,
    };

    redis_execute(tunnel, params, command).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_errors_on_missing_host() {
        let config = Config::default();
        let err = execute(config, "PING").await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("host")));
    }
}
```

- [ ] **Step 2: Wire `core::redis` into `src/core/mod.rs`**

Update `src/core/mod.rs` from:

```rust
pub mod mysql;
```

to:

```rust
pub mod mysql;
pub mod redis;
```

- [ ] **Step 3: Verify**

Run: `cargo test test_execute_errors_on_missing_host`
Expected: 2 PASS (one for mysql, one for redis).

Run: `cargo test`
Expected: 33 pass (32 prior + 1 new).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core): add core::redis::execute orchestrator

Symmetric to core::mysql::execute: validates Config (host required),
builds DirectTunnel or SshTunnel based on Config.tunnel, builds
RedisParams from Config.{password,db}, calls into
tools_mcp_redis::execute(tunnel, params, command).

CLI redis subcommand and MCP redis_exec tool (next two tasks) both
delegate here.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: CLI subcommand `tools-mcp redis "<COMMAND>"`

**Files:**
- Modify: `src/cli/args.rs` (add `Commands::Redis` variant)
- Modify: `src/cli/handler.rs` (handle the new variant)

- [ ] **Step 1: Add the `Redis` subcommand variant**

In `src/cli/args.rs`, add a new variant to the `Commands` enum (alongside the existing `Mysql` variant). The block currently looks like:

```rust
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Execute a MySQL query
    #[command(override_usage = "tools-mcp [GLOBAL OPTIONS] mysql [OPTIONS] <QUERY>")]
    Mysql {
        // ... 7 fields ...
    },
}
```

Append (do NOT remove `Mysql`):

```rust
    /// Execute a Redis command
    #[command(override_usage = "tools-mcp [GLOBAL OPTIONS] redis [OPTIONS] <COMMAND>")]
    #[command(after_help = USAGE_LEGEND)]
    Redis {
        /// Redis command to execute (e.g. "GET key" or "HSET h f1 v1").
        command: String,

        /// Redis host
        #[arg(long, help_heading = "Redis")]
        host: Option<String>,

        /// Redis port (default 6379)
        #[arg(long, help_heading = "Redis")]
        port: Option<u16>,

        /// Redis password
        #[arg(long, help_heading = "Redis")]
        password: Option<String>,

        /// Redis database number (default 0)
        #[arg(long, help_heading = "Redis")]
        db: Option<u32>,

        /// Profile name from config
        #[arg(long, help_heading = "Redis")]
        profile: Option<String>,
    },
```

(`USAGE_LEGEND` is the existing constant in `args.rs` — it's already shared with the `Mysql` variant via the same `#[command(after_help = USAGE_LEGEND)]` attribute.)

- [ ] **Step 2: Wire the handler**

In `src/cli/handler.rs`, find the `match cli.command` block in `handle()`. Currently:

```rust
match cli.command.clone() {
    Some(Commands::Mysql { query, host, port, user, password, database, profile }) => {
        let config = Self::build_config(
            &cli, ServiceType::Mysql, host, port, user, password, database,
            None,  // key_path
            profile,
        )?;
        Self::execute_mysql(&query, config).await
    }
    None => Err(Error::Config(/* ... */)),
}
```

Add a new arm for `Commands::Redis` between `Mysql` and `None`:

```rust
    Some(Commands::Redis { command, host, port, password, db, profile }) => {
        let config = Self::build_config_redis(
            &cli, host, port, password, db, profile,
        )?;
        Self::execute_redis(&command, config).await
    }
```

Then add a Redis-specific config builder (the existing `build_config` is hard-coded to MySQL fields like `user` and `database`; cleanest to add a sibling rather than generalize for Phase 5).

Append to `impl CliHandler` (next to the existing `build_config`):

```rust
    fn build_config_redis(
        cli: &Cli,
        host: Option<String>,
        port: Option<u16>,
        password: Option<String>,
        db: Option<u32>,
        profile: Option<String>,
    ) -> Result<Config> {
        let mut configs: Vec<Config> = Vec::new();

        if let Some(profile_name) = &profile {
            if let Some(toml_config) = ConfigLoader::load_default_toml()? {
                let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
                    Error::Config(format!("profile '{profile_name}' not found in config.toml"))
                })?;
                configs.push(Self::profile_to_config(profile_cfg));
            } else {
                return Err(Error::Config(format!(
                    "profile '{profile_name}' requested but no ~/.config/tools-mcp/config.toml found"
                )));
            }
        }

        if let Some(config_path) = cli.config.as_deref() {
            configs.push(ConfigLoader::load_yaml_file(config_path)?);
        }

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        configs.push(Config {
            service_type: Some(ServiceType::Redis),
            host,
            port,
            user: None,
            password,
            database: None,
            db,
            key_path: None,
            tunnel: tunnel_config,
        });

        Ok(ConfigMerger::merge_multiple(configs))
    }

    async fn execute_redis(command: &str, config: Config) -> Result<()> {
        let result = crate::core::redis::execute(config, command).await?;
        let output = CliFormatter::format(&result);
        println!("{output}");
        Ok(())
    }
```

(`profile_to_config` already copies all Profile fields including `db` — verify it does; if it doesn't, add `db: profile.db` to the field list.)

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo run -q -- redis --help 2>&1 | head -20`
Expected output includes:
```
Execute a Redis command

Usage: tools-mcp [GLOBAL OPTIONS] redis [OPTIONS] <COMMAND>

Arguments:
  <COMMAND>  Redis command to execute ...
```

Run: `cargo test`
Expected: 33 pass (no new tests in this task; CLI parsing is exercised by the build).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(cli): add 'redis <COMMAND>' subcommand

Mirrors the 'mysql' subcommand: takes a quoted command string + host/
port/password/db/profile flags, plus the global tunnel/config flags
inherited from Cli. Delegates to core::redis::execute through a thin
build_config_redis + execute_redis pair (build_config_redis is a
sibling of build_config rather than a generalization to keep the
MySQL path unchanged).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: MCP `redis_exec` tool

**Files:**
- Modify: `src/mcp/tools.rs` (add `RedisExecParams` + `redis_exec` function + `params_to_redis_config` helper)
- Modify: `src/mcp/server.rs` (register the tool)

- [ ] **Step 1: Add `RedisExecParams` + helpers in `src/mcp/tools.rs`**

Append to `src/mcp/tools.rs` (the existing module already has `MysqlExecParams`, `TunnelKind`, `SshJumpInput`, `params_to_config`, `mysql_exec`, etc.):

```rust
/// JSON parameters for the `redis_exec` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RedisExecParams {
    /// Redis command to execute (e.g. "GET key" or "HSET h f1 v1").
    pub command: String,

    /// Redis host (overrides profile / yaml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Redis port (default 6379).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Redis password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Redis database number (default 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db: Option<u32>,

    /// Profile name from ~/.config/tools-mcp/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Path to a YAML config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<PathBuf>,

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

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn redis_params_to_config(p: &RedisExecParams) -> Result<Config> {
    let mut configs: Vec<Config> = Vec::new();

    if let Some(profile_name) = &p.profile {
        let toml_config = ConfigLoader::load_default_toml()?.ok_or_else(|| {
            Error::Config(format!(
                "profile '{profile_name}' requested but no ~/.config/tools-mcp/config.toml found"
            ))
        })?;
        let profile_cfg = toml_config.profiles.get(profile_name).ok_or_else(|| {
            Error::Config(format!("profile '{profile_name}' not found in config.toml"))
        })?;
        configs.push(profile_to_config(profile_cfg));
    }

    if let Some(path) = p.config.as_deref() {
        configs.push(ConfigLoader::load_yaml_file(path)?);
    }

    // Build tunnel from the same fields as MysqlExecParams; reuse build_tunnel_config_from_fields.
    let tunnel_config = build_tunnel_config_for_redis(p)?;
    configs.push(Config {
        service_type: Some(ServiceType::Redis),
        host: p.host.clone(),
        port: p.port,
        user: None,
        password: p.password.clone(),
        database: None,
        db: p.db,
        key_path: None,
        tunnel: tunnel_config,
    });

    Ok(crate::config::ConfigMerger::merge_multiple(configs))
}

/// Same shape as the MySQL build_tunnel_config but reads from RedisExecParams.
/// Refactor opportunity: extract a shared helper taking the SSH fields by reference.
fn build_tunnel_config_for_redis(p: &RedisExecParams) -> Result<Option<TunnelConfig>> {
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
                .ok_or_else(|| Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string()))?;
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

/// Public entry point for the redis_exec tool.
pub async fn redis_exec(params: RedisExecParams) -> Result<ExecutionResult> {
    let command = params.command.clone();
    let config = redis_params_to_config(&params)?;
    crate::core::redis::execute(config, &command).await
}
```

(There is intentional duplication between `build_tunnel_config` (MySQL) and `build_tunnel_config_for_redis`. A future task could extract a shared helper taking only the SSH-related fields by reference; for Phase 5 the two functions are short and the duplication is clearer than premature abstraction.)

Add a unit test at the bottom of the existing `mod tests {}` block:

```rust
    #[test]
    fn test_redis_explicit_fields_become_config() {
        let p = RedisExecParams {
            command: "GET key".to_string(),
            host: Some("redis.internal".into()),
            port: Some(6380),
            password: Some("pwd".into()),
            db: Some(2),
            profile: None,
            config: None,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let cfg = redis_params_to_config(&p).unwrap();
        assert_eq!(cfg.host.as_deref(), Some("redis.internal"));
        assert_eq!(cfg.port, Some(6380));
        assert_eq!(cfg.password.as_deref(), Some("pwd"));
        assert_eq!(cfg.db, Some(2));
        assert_eq!(cfg.service_type, Some(ServiceType::Redis));
    }
```

- [ ] **Step 2: Register the tool in `src/mcp/server.rs`**

Add a sibling method to the existing `mysql_exec` tool. The current `impl ToolsMcpServer` block has:

```rust
    /// Execute a MySQL query, optionally through an SSH tunnel.
    #[tool(description = "Execute a MySQL query, optionally through an SSH jump host. ...")]
    async fn mysql_exec(
        &self,
        Parameters(params): Parameters<MysqlExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match mysql_exec(params).await { /* ... */ }
    }
```

Append (adjacent to it, inside the same `impl` block):

```rust
    /// Execute a Redis command, optionally through an SSH tunnel.
    #[tool(description = "Execute a Redis command, optionally through an SSH jump host. Same connection options as the `tools-mcp redis` CLI subcommand.")]
    async fn redis_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::RedisExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::redis_exec(params).await {
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

The `#[tool_router]` macro on the `impl` block automatically picks up new `#[tool(...)]`-annotated methods. No manual registration needed.

- [ ] **Step 3: Update `tests/mcp_smoke.rs` to assert redis_exec is also listed**

The existing smoke test asserts `mysql_exec` is in `tools/list`. Add a parallel assertion. In `tests/mcp_smoke.rs`, find:

```rust
    assert!(
        found_tool,
        "tools/list response did not contain mysql_exec within 10s"
    );
```

Change the `found_tool` check to track BOTH tools. Replace the search loop + assertion with:

```rust
    let mut found_mysql = false;
    let mut found_redis = false;
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
            break;
        }
    }

    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();

    assert!(found_mysql, "tools/list missing mysql_exec");
    assert!(found_redis, "tools/list missing redis_exec");
```

- [ ] **Step 4: Verify**

Run: `cargo test`
Expected: 34 pass (33 prior + 1 new redis_explicit_fields test). The mcp_smoke test still passes — it now asserts both `mysql_exec` AND `redis_exec` are in the list.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(mcp): redis_exec tool registered on the rmcp server

ToolsMcpServer now exposes both mysql_exec and redis_exec. Both go
through the same shape: JSON params -> Config (3-layer merge) ->
core::<service>::execute -> ExecutionResult JSON in a text content
block.

mcp_smoke integration test asserts both tools show up in tools/list.

Note: the build_tunnel_config helpers for MySQL and Redis are
intentional duplicates for Phase 5 readability; a shared helper can
be extracted later when a third service arrives.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: Plugin assets — `/redis` slash command + `redis-using` skill

**Files:**
- Create: `commands/redis.md`
- Create: `skills/redis-using/SKILL.md`

- [ ] **Step 1: `/redis` slash command**

Create `commands/redis.md`:

```markdown
---
name: redis
description: Run a Redis command through the tools-mcp `redis_exec` MCP tool, using the project's default profile if one is recorded.
argument-hint: <Redis command>
---

# /redis

Run this Redis command via the `redis_exec` MCP tool from the tools-mcp plugin:

```
$ARGUMENTS
```

## How to call it

1. **Pick a connection.** In order of preference:
   - If the user's CLAUDE.md / AGENTS.md / memory records a default tools-mcp Redis profile or YAML config for this project, pass it as `profile` or `config`. Don't paste the password back into the call when a profile already covers it.
   - Otherwise, ask the user once for host/port/password/db (and tunnel/ssh_*) and remember it for the rest of the session.

2. **Call the tool.** Invoke `redis_exec` with `command=$ARGUMENTS` plus the connection params from Step 1. If the user's command refers to a specific db (e.g. `SELECT 5`), don't override `db`.

3. **Render the result.** If `rows` is non-empty, format as a Markdown table with the `columns` as headers. Empty rows = the command returned `nil`; show that explicitly.

4. **Destructive commands** (`FLUSHDB`/`FLUSHALL`/`DEL`/`UNLINK` on multiple keys): pause and confirm with the user BEFORE calling the tool.

## When something fails

- Tool errors about missing host or profile not found → use the **tools-mcp-using** skill (it covers the connection / profile / tunnel pipeline shared with `mysql_exec`).
- SSH tunnel errors → use the **ssh-bastion-checklist** skill.
- Redis command errors (`WRONGTYPE`, `NOAUTH`, `MOVED` for cluster, etc.) → explain the cause to the user; suggest the right command shape if applicable.
```

- [ ] **Step 2: `redis-using` skill**

Create `skills/redis-using/SKILL.md`:

```markdown
---
name: redis-using
description: Use when calling the `redis_exec` MCP tool from the tools-mcp plugin — explains command-string syntax, the `db` parameter, output mapping for common Redis types, and when to be careful with destructive commands.
---

# Using the `redis_exec` MCP tool

`tools-mcp` exposes a `redis_exec` MCP tool symmetric to `mysql_exec`. Same connection layer (profile / YAML / explicit fields, with optional SSH tunneling); different command shape.

## Tool input

```json
{
  "command":  "GET foo",                 // required, parsed via shlex
  "host":     "redis.internal",
  "port":     6379,
  "password": "...",
  "db":       0,
  "profile":  "prod-cache",
  "tunnel":   "ssh",
  "ssh_jump": "bastion.com",
  "ssh_user": "admin"
  // ...same ssh_* / config fields as mysql_exec
}
```

`command` is the Redis CLI command as a single string. shlex parsing handles quoted args:
- `SET key value`
- `SET key "a value with spaces"`
- `HSET h f1 v1 f2 v2`
- `LPUSH list a b c`

## Three-layer config priority (low → high)

1. **TOML profile** when `profile` is set
2. **YAML file** when `config` is set
3. **Explicit fields** in the tool call (highest)

Same as `mysql_exec`. See the `tools-mcp-using` skill for the merge mechanics.

## Output mapping

`redis_exec` returns an `ExecutionResult` (`columns` + `rows` + `affected_rows`) as JSON. Phase 5 maps the Redis response types simply:

| Redis Value | columns | rows | affected_rows |
|---|---|---|---|
| Nil | `["result"]` | `[]` | 0 |
| Int(N) | `["result"]` | `[[ "N" ]]` | 1 |
| BulkString("x") | `["result"]` | `[[ "x" ]]` | 1 |
| SimpleString("OK") / Okay | `["result"]` | `[[ "OK" ]]` | 1 |
| Array([a, b, c]) | `["result"]` | `[[ "a" ], [ "b" ], [ "c" ]]` | 3 |
| Map / Set / Push / VerbatimString / etc. | `["result"]` | `[[ "<Debug-formatted>" ]]` | 1 |

So `LRANGE list 0 -1` returns one row per element. `HGETALL` returns alternating field/value rows (Redis returns a flat array; the mapping reflects that). `INFO` returns a single bulk string in one row. `EXISTS key` returns the integer.

If a user really needs structured Map/Set output (RESP3-only), the current Phase 5 mapping shows it as a Debug-formatted string in a single row — that's a known limitation. Phase 6 may add proper key-value mapping for `Map`.

## Destructive commands

`redis_exec` runs anything you give it. Confirm with the user BEFORE running:
- `FLUSHDB` / `FLUSHALL`
- `DEL` / `UNLINK` against more than a single named key
- `DEBUG FLUSHALL`
- `CONFIG SET` (server-wide changes)
- `CLUSTER FORGET` / `CLUSTER MEET`

Read-only commands (`GET`, `EXISTS`, `KEYS`, `SCAN`, `INFO`, `LRANGE`, `HGETALL`, etc.) are safe to run without a confirmation prompt.

## Common error shapes

- `Error::Config("Redis host is required")` → connection params didn't merge to a usable host. Most likely: profile doesn't exist, or no host fields anywhere.
- `Error::Service("Redis: NOAUTH ...")` → password missing or wrong.
- `Error::Service("Redis: WRONGTYPE ...")` → command applied to the wrong key type (`HGET` against a string key, etc.).
- `Error::Service("Redis: MOVED <slot> <host>:<port>")` → the key lives on a different cluster node. tools-mcp doesn't follow cluster redirects (no cluster client). Connect to the target node directly via `host`/`port` overrides.
- `Error::Execution("failed to parse Redis command (unbalanced quotes?): ...")` → shlex couldn't parse the input. Check quote balance.
- SSH errors → see the `ssh-bastion-checklist` skill.

## What this skill is NOT

- Not a Redis tutorial — assume the user knows the command they want.
- Not for cluster routing / pub-sub / transactions / scripting (Phase 6+).
- Not for `mysql_exec` — that's `tools-mcp-using`.
```

- [ ] **Step 3: Verify**

```bash
ls commands/ skills/
```
Expected: `commands/mysql.md`, `commands/redis.md`, `skills/{tools-mcp-using,mysql-debugging,ssh-bastion-checklist,redis-using}/SKILL.md`.

- [ ] **Step 4: Commit**

```bash
git add commands/redis.md skills/redis-using/
git commit -m "feat(plugin): add /redis slash command + redis-using skill

- /redis <COMMAND> — symmetric to /mysql; calls redis_exec.
- redis-using skill — documents tool input shape, three-layer config
  priority (shared with mysql_exec), output mapping for common Redis
  types, destructive-command list, and common error shapes.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Documentation + final verification

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `AGENTS.md`

- [ ] **Step 1: README — Status section**

Replace the existing `## Status` section. Update the "implemented" list and remove "Redis support" from "not yet implemented":

```markdown
## Status

This is the Phase 5 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`) and `mysql_exec` MCP tool.
- **Redis CLI mode** (`tools-mcp redis "..."`) and `redis_exec` MCP tool.
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`).
- Direct connection (`--tunnel=direct` or no `--tunnel`).
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth. Host keys accepted with a fingerprint warning.
- MCP server mode (`tools-mcp` with no subcommand) over stdio.

Not yet implemented:
- SSH direct connection (`tools-mcp ssh ...`)
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
- HTTP/SSE MCP transport
- Redis cluster routing, pub/sub, transactions, scripting (EVAL)
- Per-Value typed mapping for RESP3 `Map` / `Set` / `Push`
```

- [ ] **Step 2: README — Usage section**

Add a `### Redis` subsection after the existing `### MySQL` subsection (and before `### MCP Server`):

```markdown
### Redis

```bash
# Direct connection
tools-mcp redis "GET mykey" --host=localhost --port=6379

# With password + db
tools-mcp redis "HGETALL myhash" --host=localhost --password=secret --db=2

# Through an SSH jump
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  redis "INFO replication" --host=redis.internal --password=cache_pwd

# Using a TOML profile
tools-mcp redis "KEYS *" --profile=prod-cache
```
```

- [ ] **Step 3: README — Plugin assets list**

Update the "What the plugin provides" block in the `### Use as a Claude Code plugin` section. Replace it with:

```markdown
What the plugin provides:

- **MCP tools** auto-registered via `.mcp.json`:
  - `mysql_exec` — run a MySQL query.
  - `redis_exec` — run a Redis command.
- **Skills** that guide the assistant:
  - `tools-mcp-using` — parameter shape, three-layer config priority, multi-hop syntax.
  - `mysql-debugging` — diagnostic queries for common MySQL errors, locks, slow queries.
  - `redis-using` — Redis command shape, output mapping, destructive-command list.
  - `ssh-bastion-checklist` — narrows down SSH-tunnel failures.
- **Slash commands**:
  - `/mysql <SQL>` — quick MySQL query.
  - `/redis <COMMAND>` — quick Redis command.
```

- [ ] **Step 4: CLAUDE.md and AGENTS.md updates**

Apply identical edits to both files.

a) **Project Overview** — update the lead sentence:

Before:
```markdown
`tools-mcp` is a Rust CLI + MCP server for SSH, MySQL, and Redis. **Phase 3 (current) implements MySQL CLI mode + MCP server mode with the `mysql_exec` tool**; Redis and SSH direct are explicit phase boundaries (see below).
```

After:
```markdown
`tools-mcp` is a Rust CLI + MCP server for SSH, MySQL, and Redis. **Phase 5 (current) implements MySQL + Redis CLI modes and matching MCP tools (`mysql_exec`, `redis_exec`)**; SSH direct is the remaining service phase boundary.
```

b) **Module map** — add a new row for `tools-mcp-redis` and a row for the new orchestrator. Insert after the `tools-mcp-mysql` row:

```markdown
| `tools-mcp-redis` (lib) | `RedisConnection` (impl `core::Connection`), `RedisExecutor::run(conn, command_str)` (shlex-parsed → `redis::cmd`), and the entry `execute(tunnel, params, command_str) -> ExecutionResult`. Maps `redis::Value` → `ExecutionResult` with simple specialization for Nil/Int/BulkString/SimpleString/Okay/Array; other variants go through Debug-format. Owns the `redis` (with `tokio-comp`) + `shlex` deps. |
```

And update the existing `core::mysql::execute` row's heading to be a generic "core" row, OR add a sibling row. Recommended: replace the existing single core row with two rows:

```markdown
| `tools-mcp` bin (root `src/core/mysql.rs`) | Orchestrator `execute(Config, &str)`: validate Config, build the right tunnel, translate to `tools_mcp_mysql::MysqlParams`, call into the lib. CLI handler and MCP `mysql_exec` both delegate here. |
| `tools-mcp` bin (root `src/core/redis.rs`) | Orchestrator `execute(Config, &str)`: same shape, but builds `tools_mcp_redis::RedisParams` from `Config.{password,db}` and dispatches to the redis lib. |
```

c) **Phase boundaries** — update. Before:

```markdown
- **Redis / SSH-direct subcommands**: not yet implemented. When added, mirror the existing pattern: a `core::<service>` execution function, a CLI subcommand under `cli::Commands`, and an MCP tool in `mcp::tools` that delegates to the core. ...
```

After:

```markdown
- **Redis subcommand**: implemented in Phase 5. `tools-mcp redis "..."` and the `redis_exec` MCP tool both route through `core::redis::execute`. The `db` field is on `Config`/`Profile` for the Redis database number.
- **SSH-direct subcommand**: not yet implemented. When added, mirror the existing pattern: a `core::ssh` execution function, a CLI subcommand under `cli::Commands`, and an MCP tool in `mcp::tools` that delegates to the core. CLI and MCP must share the core; never duplicate execution logic in MCP land.
```

d) **Conventions worth knowing** — append a new bullet:

```markdown
- **Per-service config fields**: `Config` is a flat bag of all possible fields across services (`database` for MySQL, `db` for Redis, `key_path` for SSH, etc.). Each orchestrator picks out only what it needs. When adding a new service that requires a new field, add it to `Profile` and `Config` (and the `ConfigMerger::merge` `or` chain) — but only if existing fields can't carry the meaning.
```

- [ ] **Step 5: Verify both files differ only on the cross-link**

Run: `diff <(tail -n +5 CLAUDE.md) <(tail -n +5 AGENTS.md)`
Expected: only the cross-link blockquote line + pre-existing methodology trailer line difference.

- [ ] **Step 6: Final verification**

Run: `cargo test`
Expected: 34 pass (Phase 4 baseline 26 + 1 redis connection + 5 value mapping + 1 redis core + 1 redis params).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean.

Run: `cargo build --release`
Expected: workspace builds; binary at `target/release/tools-mcp`.

Run: `./target/release/tools-mcp redis --help | head -3`
Expected:
```
Execute a Redis command

Usage: tools-mcp [GLOBAL OPTIONS] redis [OPTIONS] <COMMAND>
```

- [ ] **Step 7: Commit**

```bash
git add README.md CLAUDE.md AGENTS.md
git commit -m "docs: document Phase 5 Redis support

- README Status: Phase 5 with mysql_exec + redis_exec; Redis usage
  examples; updated plugin asset list to include /redis + redis-using.
- CLAUDE.md / AGENTS.md: lead sentence updated; module map adds
  tools-mcp-redis lib and core::redis orchestrator rows; Phase
  boundaries record Redis as shipped (only SSH-direct remains);
  conventions add a 'per-service config fields' note.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Summary

After Phase 5:

- `tools-mcp redis "..."` works as a CLI subcommand, with the same global tunnel/config/profile flags as MySQL.
- The `redis_exec` MCP tool exposes the same surface to AI clients.
- Both share the orchestrator at `core::redis::execute`.
- A new `tools-mcp-redis` lib crate owns the `redis` + `shlex` deps; `tools-mcp-core` and `tools-mcp-mysql` are unchanged.
- The plugin ships a `/redis` slash command + a `redis-using` skill alongside the existing `/mysql` and skills.
- All 34 tests pass; clippy clean; release build clean.
- Architecture remains: every CLI subcommand has a paired MCP tool; both delegate to a `core::<service>::execute` orchestrator that translates `Config` to a service lib's params struct.

**Deferred to Phase 6+:** SSH-direct subcommand + tool, Redis cluster routing, Redis pub/sub / transactions / scripting, RESP3 typed `Map`/`Set` mapping, HTTP/SSE MCP transport.
