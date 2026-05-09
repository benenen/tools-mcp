# Tools MCP Phase 9: PostgreSQL + MongoDB Support

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to execute this plan task-by-task.

**Goal:** Add `tools-mcp-pgsql` and `tools-mcp-mongo` lib crates modeled on `tools-mcp-mysql`, plus matching `PgsqlOrchestrator` / `MongoOrchestrator` (impl `Service` via typed `<Svc>Request::from_config`), CLI subcommands `tools-mcp pgsql "..."` / `tools-mcp mongo "..."`, and MCP tools `pgsql_exec` / `mongo_exec`. Fits the Phase 8 architecture exactly — every new piece mirrors the mysql pattern.

**Architecture:**
- Postgres uses `tokio-postgres = "0.7"` (lightweight; spawn the Connection task; `client.query(sql, &[])` for simple statements).
- Mongo uses `mongodb = "3"` with the JSON-doc → `db.run_command(doc)` model. Caller passes a JSON string; we parse to BSON, run, serialize the result Document back to JSON for `ExecutionResult`.
- Both services support Profile/YAML 3-layer merge (mysql/redis pattern), so `ServiceType` gains `Pgsql` and `Mongo` variants.

**Tech Stack:**
- New deps: `tokio-postgres = "0.7"` (with `with-chrono-0_4` feature for timestamp display), `mongodb = "3"`, `bson = "2"` (mongodb re-exports it but explicit is safer for serde).
- Existing deps reused: `async-trait`, `serde`, `tools-mcp-core`.

**Out of scope (future phases):**
- TLS for postgres / mongo (NoTls is fine for MVP; SSH tunnel + private network covers most use cases).
- Postgres COPY / LISTEN/NOTIFY / prepared statements.
- Mongo cursor pagination beyond the first batch (the `cursor.firstBatch` is what `run_command` returns; subsequent batches require a separate `getMore` cycle that's out of scope).
- Connection pooling beyond what each driver provides by default.

---

## File Structure

```
crates/
├── tools-mcp-pgsql/                          (NEW)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── connection.rs                     PgsqlConnection (impl core::Connection)
│       ├── executor.rs                       PgsqlExecutor (row → ExecutionResult)
│       └── execute.rs                        PgsqlParams + execute(tunnel, params, sql)
└── tools-mcp-mongo/                          (NEW)
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── connection.rs                     MongoConnection (impl core::Connection)
        ├── executor.rs                       MongoExecutor (Document → ExecutionResult)
        └── execute.rs                        MongoParams + execute(tunnel, params, json_cmd)
```

In `tools-mcp-orchestrator`:

```
crates/tools-mcp-orchestrator/src/
├── pgsql.rs                                  (NEW) PgsqlRequest + Orchestrator
├── mongo.rs                                  (NEW) MongoRequest + Orchestrator
└── lib.rs                                    add pub mod pgsql/mongo + re-exports
```

In bin:

```
src/cli/args.rs                                add Commands::Pgsql + Commands::Mongo
src/cli/handler.rs                             add execute_pgsql + execute_mongo + build_config_pgsql + build_config_mongo
src/mcp/server.rs                              add #[tool] pgsql_exec + mongo_exec
src/mcp/tools.rs                               add PgsqlExecParams / pgsql_exec / MongoExecParams / mongo_exec
```

---

## Task 1: Bootstrap `tools-mcp-pgsql` crate

**Files:**
- Create: `crates/tools-mcp-pgsql/Cargo.toml`
- Create: `crates/tools-mcp-pgsql/src/lib.rs`
- Create: `crates/tools-mcp-pgsql/src/connection.rs`
- Create: `crates/tools-mcp-pgsql/src/executor.rs`
- Create: `crates/tools-mcp-pgsql/src/execute.rs`
- Modify: root `Cargo.toml` workspace members (alphabetical insertion: between `mysql` and `redis` — wait, between `orchestrator` and `redis` actually since orchestrator is in slot 4).

- [ ] **Step 1: Cargo.toml**

```toml
[package]
name = "tools-mcp-pgsql"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
chrono = "0.4"
tokio-postgres = { version = "0.7", features = ["with-chrono-0_4"] }
tools-mcp-core = { path = "../tools-mcp-core" }

[dev-dependencies]
tokio = { version = "1.40", features = ["full"] }
```

- [ ] **Step 2: `src/lib.rs`**

```rust
//! PostgreSQL connection + executor primitives, layered on `tools-mcp-core`.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::PgsqlConnection;
pub use execute::{PgsqlParams, execute};
pub use executor::PgsqlExecutor;
```

- [ ] **Step 3: `src/connection.rs`**

```rust
use async_trait::async_trait;
use tokio_postgres::{Client, Config, NoTls};
use tools_mcp_core::{Connection, Error, Result, Tunnel};

pub struct PgsqlConnection {
    tunnel: Box<dyn Tunnel>,
    user: String,
    password: Option<String>,
    database: Option<String>,
    client: Option<Client>,
}

impl PgsqlConnection {
    pub fn new(
        tunnel: Box<dyn Tunnel>,
        user: String,
        password: Option<String>,
        database: Option<String>,
    ) -> Self {
        Self {
            tunnel,
            user,
            password,
            database,
            client: None,
        }
    }

    pub fn client(&mut self) -> Result<&mut Client> {
        self.client
            .as_mut()
            .ok_or_else(|| Error::Connection("Pgsql connection not established".to_string()))
    }
}

#[async_trait]
impl Connection for PgsqlConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;

        let mut cfg = Config::new();
        cfg.host(&endpoint.host)
            .port(endpoint.port)
            .user(&self.user);
        if let Some(ref pw) = self.password {
            cfg.password(pw);
        }
        if let Some(ref db) = self.database {
            cfg.dbname(db);
        }

        let (client, connection) = cfg
            .connect(NoTls)
            .await
            .map_err(|e| Error::Service(format!("Pgsql: {e}")))?;

        // Background driver task — drops when client is dropped.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("pgsql connection task error: {e}");
            }
        });

        self.client = Some(client);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            drop(client);
        }
        self.tunnel.close().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.client.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools_mcp_core::TunnelEndpoint;

    struct TestTunnel { active: bool }

    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint { host: "localhost".to_string(), port: 5432 })
        }
        async fn close(&mut self) -> Result<()> { self.active = false; Ok(()) }
        fn is_active(&self) -> bool { self.active }
    }

    #[tokio::test]
    async fn test_pgsql_connection_new() {
        let t = Box::new(TestTunnel { active: false });
        let c = PgsqlConnection::new(t, "u".to_string(), Some("p".to_string()), None);
        assert!(!c.is_connected());
    }
}
```

- [ ] **Step 4: `src/executor.rs`**

```rust
use crate::connection::PgsqlConnection;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use tokio_postgres::{Row, types::Type};
use tools_mcp_core::{Error, ExecutionResult, Result};

pub struct PgsqlExecutor;

impl PgsqlExecutor {
    pub async fn execute(conn: &mut PgsqlConnection, query: &str) -> Result<ExecutionResult> {
        let client = conn.client()?;

        let rows: Vec<Row> = client
            .query(query, &[])
            .await
            .map_err(|e| Error::Service(format!("Pgsql query: {e}")))?;

        if rows.is_empty() {
            return Ok(ExecutionResult::new(vec![], vec![], 0));
        }

        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let str_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| Self::col_to_string(row, i))
                    .collect()
            })
            .collect();

        let n = str_rows.len() as u64;
        Ok(ExecutionResult::new(columns, str_rows, n))
    }

    fn col_to_string(row: &Row, i: usize) -> String {
        let col = &row.columns()[i];
        let ty = col.type_();
        // Each branch: try_get returns Result<Option<T>, Error>. Map None → "NULL",
        // Some(v) → display, Err → fallback string.
        macro_rules! show_opt {
            ($t:ty) => {
                match row.try_get::<_, Option<$t>>(i) {
                    Ok(Some(v)) => v.to_string(),
                    Ok(None) => "NULL".to_string(),
                    Err(e) => format!("<{}: {e}>", ty.name()),
                }
            };
        }

        match *ty {
            Type::BOOL => show_opt!(bool),
            Type::INT2 => show_opt!(i16),
            Type::INT4 => show_opt!(i32),
            Type::INT8 => show_opt!(i64),
            Type::FLOAT4 => show_opt!(f32),
            Type::FLOAT8 => show_opt!(f64),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => show_opt!(String),
            Type::UUID => match row.try_get::<_, Option<uuid::Uuid>>(i) {
                // tokio-postgres + uuid feature would do it cleanly; we don't enable
                // it for MVP — fall through to bytea-as-debug.
                _ => format!("<{}>", ty.name()),
            },
            Type::DATE => show_opt!(NaiveDate),
            Type::TIME => show_opt!(NaiveTime),
            Type::TIMESTAMP => show_opt!(NaiveDateTime),
            Type::TIMESTAMPTZ => show_opt!(DateTime<Utc>),
            Type::JSON | Type::JSONB => match row.try_get::<_, Option<serde_json::Value>>(i) {
                Ok(Some(v)) => v.to_string(),
                Ok(None) => "NULL".to_string(),
                Err(e) => format!("<{}: {e}>", ty.name()),
            },
            _ => format!("<{}>", ty.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pgsql_executor_new() {
        let _e = PgsqlExecutor;
    }
}
```

**Note on uuid / json:** the macro path with `uuid::Uuid` requires the `uuid` crate. To avoid extra deps for the MVP, use `format!("<{}>", ty.name())` for `UUID` (i.e. drop the `uuid::Uuid` arm to a fallback). For JSON/JSONB, `serde_json` is already a transitive dep of tokio-postgres → keep that arm, but if `serde_json::Value` isn't directly accessible without a feature flag, fall back to `format!("<{}>", ty.name())` for those types too. The implementer should pick the lightest path that compiles cleanly; the `<typename>` fallback is always acceptable.

- [ ] **Step 5: `src/execute.rs`**

```rust
//! Top-level entry: build a Pgsql connection over the supplied tunnel,
//! run a single query, and return the structured result.

use tools_mcp_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::PgsqlConnection;
use crate::executor::PgsqlExecutor;

#[derive(Debug, Clone)]
pub struct PgsqlParams {
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: PgsqlParams,
    query: &str,
) -> Result<ExecutionResult> {
    let mut conn = PgsqlConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = PgsqlExecutor::execute(&mut conn, query).await;
    let _ = conn.disconnect().await;
    exec_result
}
```

- [ ] **Step 6: Workspace member**

In root `Cargo.toml [workspace] members`, add `"crates/tools-mcp-pgsql"` (alphabetical — between `tools-mcp-orchestrator` and `tools-mcp-redis`).

- [ ] **Step 7: Verify + commit**

```
cargo build -p tools-mcp-pgsql
cargo test -p tools-mcp-pgsql
cargo clippy -p tools-mcp-pgsql --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: green. test count = 2 (connection + executor stubs).

```bash
git add -A
git commit -m "feat(pgsql): add tools-mcp-pgsql crate

Lightweight wrapper around tokio-postgres modeled on tools-mcp-mysql:
PgsqlConnection (impl core::Connection), PgsqlExecutor (Row → strings),
execute(tunnel, params, query) entry. PostgreSQL types covered:
bool/int2/4/8, float4/8, text/varchar/bpchar/name, date/time/timestamp(tz),
json/jsonb. Uncommon types fall back to <typename>.

NoTls for now — SSH tunnel + private network covers MVP.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Bootstrap `tools-mcp-mongo` crate

**Files:**
- Create: `crates/tools-mcp-mongo/Cargo.toml`
- Create: `crates/tools-mcp-mongo/src/lib.rs`
- Create: `crates/tools-mcp-mongo/src/connection.rs`
- Create: `crates/tools-mcp-mongo/src/executor.rs`
- Create: `crates/tools-mcp-mongo/src/execute.rs`
- Modify: workspace `Cargo.toml`

- [ ] **Step 1: Cargo.toml**

```toml
[package]
name = "tools-mcp-mongo"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
mongodb = { version = "3", default-features = false, features = ["rustls-tls"] }
serde_json = "1"
tools-mcp-core = { path = "../tools-mcp-core" }

[dev-dependencies]
tokio = { version = "1.40", features = ["full"] }
```

(`rustls-tls` is the modern default; the mongodb crate still supports plain TCP via the connection string, so this enables TLS without forcing it.)

- [ ] **Step 2: `src/lib.rs`**

```rust
//! MongoDB connection + executor primitives, layered on `tools-mcp-core`.
//!
//! Commands are JSON documents passed to `Database::run_command`. The
//! returned BSON Document is serialized to JSON and presented as a single
//! `result` row, matching the redis_exec mapping convention for non-table
//! results.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::MongoConnection;
pub use execute::{MongoParams, execute};
pub use executor::MongoExecutor;
```

- [ ] **Step 3: `src/connection.rs`**

The mongo client builds from a connection string. Use `mongodb::options::ClientOptions::parse_connection_string_async` (3.x API; older versions use `parse`). When in doubt, look at `mongodb`'s docs or examples in target/doc after `cargo doc -p mongodb --open`. Skeleton:

```rust
use async_trait::async_trait;
use mongodb::{Client, options::ClientOptions};
use tools_mcp_core::{Connection, Error, Result, Tunnel};

pub struct MongoConnection {
    tunnel: Box<dyn Tunnel>,
    user: Option<String>,
    password: Option<String>,
    database: String,
    client: Option<Client>,
}

impl MongoConnection {
    pub fn new(
        tunnel: Box<dyn Tunnel>,
        user: Option<String>,
        password: Option<String>,
        database: String,
    ) -> Self {
        Self { tunnel, user, password, database, client: None }
    }

    pub fn client(&self) -> Result<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| Error::Connection("Mongo connection not established".to_string()))
    }

    pub fn database_name(&self) -> &str {
        &self.database
    }
}

#[async_trait]
impl Connection for MongoConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;

        // Build URI: mongodb://[user:pass@]host:port
        let auth = match (&self.user, &self.password) {
            (Some(u), Some(p)) => format!(
                "{}:{}@",
                urlencoding::encode(u),
                urlencoding::encode(p)
            ),
            (Some(u), None) => format!("{}@", urlencoding::encode(u)),
            _ => String::new(),
        };
        let uri = format!("mongodb://{auth}{}:{}", endpoint.host, endpoint.port);

        let opts = ClientOptions::parse(&uri)
            .await
            .map_err(|e| Error::Service(format!("Mongo: {e}")))?;
        let client = Client::with_options(opts)
            .map_err(|e| Error::Service(format!("Mongo: {e}")))?;

        self.client = Some(client);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            drop(client);
        }
        self.tunnel.close().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.client.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools_mcp_core::TunnelEndpoint;

    struct TestTunnel { active: bool }
    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint { host: "localhost".to_string(), port: 27017 })
        }
        async fn close(&mut self) -> Result<()> { self.active = false; Ok(()) }
        fn is_active(&self) -> bool { self.active }
    }

    #[tokio::test]
    async fn test_mongo_connection_new() {
        let t = Box::new(TestTunnel { active: false });
        let c = MongoConnection::new(t, Some("u".into()), Some("p".into()), "test".into());
        assert!(!c.is_connected());
    }
}
```

**Note**: requires `urlencoding = "2"` as a dep. Add it to Cargo.toml (alphabetical).

- [ ] **Step 4: `src/executor.rs`**

```rust
use crate::connection::MongoConnection;
use mongodb::bson::{Document, Bson};
use tools_mcp_core::{Error, ExecutionResult, Result};

pub struct MongoExecutor;

impl MongoExecutor {
    /// Parse `command_str` as JSON, convert to BSON, run on the configured
    /// database via `run_command`, and serialize the result Document back
    /// to JSON for an ExecutionResult.
    pub async fn execute(
        conn: &mut MongoConnection,
        command_str: &str,
    ) -> Result<ExecutionResult> {
        let json: serde_json::Value = serde_json::from_str(command_str).map_err(|e| {
            Error::Execution(format!("failed to parse Mongo command as JSON: {e}"))
        })?;

        let bson_val: Bson = json.try_into().map_err(|e| {
            Error::Execution(format!("failed to convert command JSON to BSON: {e}"))
        })?;
        let cmd_doc: Document = match bson_val {
            Bson::Document(d) => d,
            _ => {
                return Err(Error::Execution(
                    "Mongo command must be a JSON object".to_string(),
                ));
            }
        };

        let client = conn.client()?;
        let db = client.database(conn.database_name());
        let result_doc: Document = db
            .run_command(cmd_doc)
            .await
            .map_err(|e| Error::Service(format!("Mongo run_command: {e}")))?;

        let result_json = serde_json::to_string(&result_doc)
            .map_err(|e| Error::Service(format!("Mongo result serialization: {e}")))?;

        Ok(ExecutionResult::new(
            vec!["result".to_string()],
            vec![vec![result_json]],
            1,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mongo_executor_new() {
        let _e = MongoExecutor;
    }
}
```

**Note on the `try_into()` path**: `serde_json::Value` → `Bson` is provided by mongodb's bson crate via a `From` impl. If that doesn't compile cleanly in 3.x, fall back to going through string: serialize JSON to a `&str`, then `Document::from_iter`-style or use `mongodb::bson::to_document` after first deserializing into a HashMap. The implementer should pick whichever yields a clean compile.

- [ ] **Step 5: `src/execute.rs`**

```rust
//! Top-level entry: build a Mongo connection over the supplied tunnel,
//! run a single command, return the structured result.

use tools_mcp_core::{Connection, ExecutionResult, Result, Tunnel};

use crate::connection::MongoConnection;
use crate::executor::MongoExecutor;

#[derive(Debug, Clone)]
pub struct MongoParams {
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: String,
}

pub async fn execute(
    tunnel: Box<dyn Tunnel>,
    params: MongoParams,
    command_str: &str,
) -> Result<ExecutionResult> {
    let mut conn = MongoConnection::new(tunnel, params.user, params.password, params.database);
    let exec_result = MongoExecutor::execute(&mut conn, command_str).await;
    let _ = conn.disconnect().await;
    exec_result
}
```

- [ ] **Step 6: Workspace member**

Add `"crates/tools-mcp-mongo"` to workspace members (alphabetical — between `tools-mcp-http` and `tools-mcp-mysql`).

- [ ] **Step 7: Verify + commit**

```
cargo build -p tools-mcp-mongo
cargo test -p tools-mcp-mongo
cargo clippy -p tools-mcp-mongo --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: green. test count = 2.

If the `serde_json::Value → Bson` conversion path doesn't compile, mark as DONE_WITH_CONCERNS and switch to `mongodb::bson::to_document(&json)?` (going through serde) as a fallback.

```bash
git add -A
git commit -m "feat(mongo): add tools-mcp-mongo crate

Lightweight wrapper around the mongodb 3.x driver. Commands are JSON
documents passed to Database::run_command; the returned BSON Document
is serialized back to JSON for a single result row. Modeled on
tools-mcp-mysql + tools-mcp-redis.

Auth via URL-encoded user:password in the connection URI. Connection
string assembled from the resolved tunnel endpoint.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Add `Pgsql` and `Mongo` to `ServiceType`

**Files:**
- Modify: `crates/tools-mcp-orchestrator/src/config/types.rs`

- [ ] **Step 1: Extend `ServiceType` enum**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Mysql,
    Pgsql,
    Redis,
    Mongo,
    Ssh,
    Http,
}
```

- [ ] **Step 2: Extend `FromStr`**

```rust
match s.to_lowercase().as_str() {
    "mysql" => Ok(ServiceType::Mysql),
    "pgsql" | "postgres" | "postgresql" => Ok(ServiceType::Pgsql),
    "redis" => Ok(ServiceType::Redis),
    "mongo" | "mongodb" => Ok(ServiceType::Mongo),
    "ssh" => Ok(ServiceType::Ssh),
    "http" => Ok(ServiceType::Http),
    _ => Err(format!("Invalid service type: {}", s)),
}
```

(Aliases: `pgsql` is the canonical name to match the crate; `postgres`/`postgresql` are accepted for ergonomics. `mongo`/`mongodb` similarly.)

- [ ] **Step 3: Verify**

```
cargo test -p tools-mcp-orchestrator
cargo build
```

Existing `test_service_type_from_str` should still pass; the implementer should also add an inline assertion for the new variants:

```rust
assert_eq!("pgsql".parse::<ServiceType>().unwrap(), ServiceType::Pgsql);
assert_eq!("postgres".parse::<ServiceType>().unwrap(), ServiceType::Pgsql);
assert_eq!("mongo".parse::<ServiceType>().unwrap(), ServiceType::Mongo);
assert_eq!("mongodb".parse::<ServiceType>().unwrap(), ServiceType::Mongo);
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(orchestrator): add Pgsql + Mongo to ServiceType

Aliases: pgsql/postgres/postgresql, mongo/mongodb.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `PgsqlOrchestrator` + typed `PgsqlRequest`

**Files:**
- Create: `crates/tools-mcp-orchestrator/src/pgsql.rs`
- Modify: `crates/tools-mcp-orchestrator/Cargo.toml` (add `tools-mcp-pgsql`)
- Modify: `crates/tools-mcp-orchestrator/src/lib.rs`

- [ ] **Step 1: Add dep**

In orchestrator Cargo.toml `[dependencies]` (alphabetical, between `tools-mcp-mysql` and `tools-mcp-redis` — wait, between `tools-mcp-orchestrator-...` no — between `tools-mcp-mysql` and `tools-mcp-redis`):

```toml
tools-mcp-pgsql = { path = "../tools-mcp-pgsql" }
```

- [ ] **Step 2: Create `src/pgsql.rs`**

Mirror `src/mysql.rs` exactly. Field set: same (host/port/user/password/database/query). `from_config`: same validation (host required, user required), default port = 5432.

```rust
//! Pgsql orchestrator: typed request → `tools_mcp_pgsql::execute`.

use crate::config::Config;
use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools_mcp_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools_mcp_pgsql::{PgsqlParams, execute as pgsql_execute};

#[derive(Debug, Clone)]
pub struct PgsqlRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub query: String,
}

impl PgsqlRequest {
    pub fn from_config(config: Config, query: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("Pgsql host is required".to_string()))?;
        let port = config.port.unwrap_or(5432);
        let user = config
            .user
            .ok_or_else(|| Error::Config("Pgsql user is required".to_string()))?;
        Ok(PgsqlRequest {
            host,
            port,
            user,
            password: config.password,
            database: config.database,
            query,
        })
    }
}

pub struct PgsqlOrchestrator;

#[async_trait]
impl Service for PgsqlOrchestrator {
    type Request = PgsqlRequest;

    async fn execute(
        req: PgsqlRequest,
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

        let params = PgsqlParams {
            user: req.user,
            password: req.password,
            database: req.database,
        };
        pgsql_execute(tunnel, params, &req.query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_config_errors_on_missing_host() {
        let cfg = Config { user: Some("u".into()), ..Default::default() };
        let err = PgsqlRequest::from_config(cfg, "SELECT 1".into()).unwrap_err();
        assert!(matches!(err, Error::Config(m) if m.contains("host")));
    }

    #[test]
    fn test_from_config_errors_on_missing_user() {
        let cfg = Config { host: Some("h".into()), ..Default::default() };
        let err = PgsqlRequest::from_config(cfg, "SELECT 1".into()).unwrap_err();
        assert!(matches!(err, Error::Config(m) if m.contains("user")));
    }

    #[test]
    fn test_from_config_succeeds() {
        let cfg = Config {
            host: Some("h".into()),
            user: Some("u".into()),
            password: Some("p".into()),
            database: Some("d".into()),
            port: Some(5433),
            ..Default::default()
        };
        let req = PgsqlRequest::from_config(cfg, "SELECT 1".into()).unwrap();
        assert_eq!(req.host, "h");
        assert_eq!(req.port, 5433);
    }

    #[test]
    fn test_from_config_defaults_port() {
        let cfg = Config { host: Some("h".into()), user: Some("u".into()), ..Default::default() };
        let req = PgsqlRequest::from_config(cfg, "SELECT 1".into()).unwrap();
        assert_eq!(req.port, 5432);
    }
}
```

- [ ] **Step 3: Wire into `src/lib.rs`**

```rust
pub mod config;
pub mod http;
pub mod mongo;        // added later in Task 5
pub mod mysql;
pub mod pgsql;
pub mod redis;
pub mod ssh;
pub mod tunnel;

pub use http::HttpOrchestrator;
pub use mongo::{MongoOrchestrator, MongoRequest};
pub use mysql::{MysqlOrchestrator, MysqlRequest};
pub use pgsql::{PgsqlOrchestrator, PgsqlRequest};
pub use redis::{RedisOrchestrator, RedisRequest};
pub use ssh::SshDirectOrchestrator;

pub use tools_mcp_http::{HttpAuth, HttpRequestSpec};
pub use tools_mcp_ssh::SshExecRequest;
```

(In Task 4 only `pgsql` is added; `mongo` lands in Task 5.)

- [ ] **Step 4: Verify + commit**

```
cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check
```

```bash
git commit -am "feat(orchestrator): PgsqlOrchestrator impl Service via PgsqlRequest

Mirrors MysqlOrchestrator exactly — typed PgsqlRequest with from_config
constructor (validates host + user, defaults port 5432). Test count +4.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: `MongoOrchestrator` + typed `MongoRequest`

**Files:**
- Create: `crates/tools-mcp-orchestrator/src/mongo.rs`
- Modify: `crates/tools-mcp-orchestrator/Cargo.toml` (add `tools-mcp-mongo`)
- Modify: `crates/tools-mcp-orchestrator/src/lib.rs` (add `pub mod mongo;` + re-export)

- [ ] **Step 1: Add dep**

```toml
tools-mcp-mongo = { path = "../tools-mcp-mongo" }
```

(Alphabetical — between `tools-mcp-http` and `tools-mcp-mysql`.)

- [ ] **Step 2: Create `src/mongo.rs`**

```rust
//! Mongo orchestrator: typed request → `tools_mcp_mongo::execute`.

use crate::config::Config;
use crate::tunnel::{DirectTunnel, SshTunnel};
use async_trait::async_trait;
use tools_mcp_core::{Error, ExecutionResult, Result, Service, Tunnel, TunnelConfig};
use tools_mcp_mongo::{MongoParams, execute as mongo_execute};

#[derive(Debug, Clone)]
pub struct MongoRequest {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: String,
    pub command: String,
}

impl MongoRequest {
    /// Validates host + database (both required). Mongo auth is optional —
    /// neither user nor password is required by `from_config` itself
    /// (the server may reject unauthenticated connections at runtime).
    /// Default port is 27017.
    pub fn from_config(config: Config, command: String) -> Result<Self> {
        let host = config
            .host
            .ok_or_else(|| Error::Config("Mongo host is required".to_string()))?;
        let port = config.port.unwrap_or(27017);
        let database = config
            .database
            .ok_or_else(|| Error::Config("Mongo database is required".to_string()))?;
        Ok(MongoRequest {
            host,
            port,
            user: config.user,
            password: config.password,
            database,
            command,
        })
    }
}

pub struct MongoOrchestrator;

#[async_trait]
impl Service for MongoOrchestrator {
    type Request = MongoRequest;

    async fn execute(
        req: MongoRequest,
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

        let params = MongoParams {
            user: req.user,
            password: req.password,
            database: req.database,
        };
        mongo_execute(tunnel, params, &req.command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_config_errors_on_missing_host() {
        let cfg = Config { database: Some("d".into()), ..Default::default() };
        let err = MongoRequest::from_config(cfg, "{}".into()).unwrap_err();
        assert!(matches!(err, Error::Config(m) if m.contains("host")));
    }

    #[test]
    fn test_from_config_errors_on_missing_database() {
        let cfg = Config { host: Some("h".into()), ..Default::default() };
        let err = MongoRequest::from_config(cfg, "{}".into()).unwrap_err();
        assert!(matches!(err, Error::Config(m) if m.contains("database")));
    }

    #[test]
    fn test_from_config_succeeds() {
        let cfg = Config {
            host: Some("h".into()),
            database: Some("d".into()),
            user: Some("u".into()),
            password: Some("p".into()),
            port: Some(27018),
            ..Default::default()
        };
        let req = MongoRequest::from_config(cfg, "{}".into()).unwrap();
        assert_eq!(req.port, 27018);
        assert_eq!(req.database, "d");
        assert_eq!(req.user.as_deref(), Some("u"));
    }

    #[test]
    fn test_from_config_defaults_port() {
        let cfg = Config {
            host: Some("h".into()),
            database: Some("d".into()),
            ..Default::default()
        };
        let req = MongoRequest::from_config(cfg, "{}".into()).unwrap();
        assert_eq!(req.port, 27017);
    }
}
```

- [ ] **Step 3: Update `src/lib.rs`** to add `pub mod mongo;` + `pub use mongo::{MongoOrchestrator, MongoRequest};` (see Task 4 step 3 for the final layout).

- [ ] **Step 4: Verify + commit**

```bash
git commit -am "feat(orchestrator): MongoOrchestrator impl Service via MongoRequest

Mirrors PgsqlOrchestrator. Validates host + database (both required;
mongo auth is optional). Default port 27017. Test count +4.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: CLI subcommands + handlers (`pgsql`, `mongo`)

**Files:**
- Modify: `src/cli/args.rs` — extend `Commands` enum
- Modify: `src/cli/handler.rs` — `execute_pgsql`, `execute_mongo`, `build_config_pgsql`, `build_config_mongo`

- [ ] **Step 1: Extend `Commands` enum in `src/cli/args.rs`**

Add two variants modeled after `Commands::Mysql` (which is the closest match — both take a query/command string + optional host/port/user/password/database/profile/config + the global tunnel/ssh args via `SshTunnelArgs` flatten):

```rust
/// Execute a PostgreSQL query
Pgsql {
    /// SQL query to execute
    query: String,

    #[arg(long, conflicts_with = "profile")]
    host: Option<String>,
    #[arg(long, conflicts_with = "profile")]
    port: Option<u16>,
    #[arg(long, conflicts_with = "profile")]
    user: Option<String>,
    #[arg(long, conflicts_with = "profile")]
    password: Option<String>,
    #[arg(long, conflicts_with = "profile")]
    database: Option<String>,
    #[arg(long)]
    profile: Option<String>,
},

/// Execute a MongoDB command (JSON document passed to db.runCommand)
Mongo {
    /// MongoDB command as a JSON object (e.g. `{"find":"users","filter":{}}`)
    command: String,

    #[arg(long, conflicts_with = "profile")]
    host: Option<String>,
    #[arg(long, conflicts_with = "profile")]
    port: Option<u16>,
    #[arg(long, conflicts_with = "profile")]
    user: Option<String>,
    #[arg(long, conflicts_with = "profile")]
    password: Option<String>,
    #[arg(long, conflicts_with = "profile")]
    database: Option<String>,
    #[arg(long)]
    profile: Option<String>,
},
```

(Match the existing Mysql variant's exact `#[arg(...)]` shape — including `after_help` if it has one. Reading `args.rs` first to see the pattern is easier than describing it here.)

- [ ] **Step 2: Wire dispatch in `CliHandler::handle`**

In the `match cli.command` block alongside `Some(Commands::Mysql { ... })`, add:

```rust
Some(Commands::Pgsql { query, host, port, user, password, database, profile }) => {
    let config = Self::build_config_pgsql(&cli, host, port, user, password, database, profile)?;
    Self::execute_pgsql(&query, config).await
}
Some(Commands::Mongo { command, host, port, user, password, database, profile }) => {
    let config = Self::build_config_mongo(&cli, host, port, user, password, database, profile)?;
    Self::execute_mongo(&command, config).await
}
```

- [ ] **Step 3: Implement `build_config_pgsql` + `build_config_mongo`**

These are mechanical copies of `build_config_redis` (renamed + with `ServiceType::Pgsql` / `Mongo` in the explicit-fields layer).

- [ ] **Step 4: Implement `execute_pgsql` + `execute_mongo`**

Mirror `execute_mysql`:

```rust
async fn execute_pgsql(query: &str, config: Config) -> Result<()> {
    let tunnel = config.tunnel.clone();
    let req = PgsqlRequest::from_config(config, query.to_string())?;
    let result = PgsqlOrchestrator::execute(req, tunnel).await?;
    let output = CliFormatter::format(&result);
    println!("{output}");
    Ok(())
}

async fn execute_mongo(command: &str, config: Config) -> Result<()> {
    let tunnel = config.tunnel.clone();
    let req = MongoRequest::from_config(config, command.to_string())?;
    let result = MongoOrchestrator::execute(req, tunnel).await?;
    let output = CliFormatter::format(&result);
    println!("{output}");
    Ok(())
}
```

Add `PgsqlOrchestrator, PgsqlRequest, MongoOrchestrator, MongoRequest` to the existing `use tools_mcp_orchestrator::{...};` import.

- [ ] **Step 5: Verify + smoke**

```
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
./target/debug/tools-mcp pgsql --help | head -3
./target/debug/tools-mcp mongo --help | head -3
```

Expected: both subcommands print Usage lines.

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(cli): add pgsql + mongo subcommands

Mirrors the mysql subcommand shape — query/command + host/port/user/
password/database/profile, plus the global tunnel/ssh-* flags. Both
delegate to PgsqlOrchestrator::execute / MongoOrchestrator::execute
via from_config validation.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: MCP tools (`pgsql_exec`, `mongo_exec`)

**Files:**
- Modify: `src/mcp/tools.rs` — add `PgsqlExecParams`, `pgsql_exec`, `MongoExecParams`, `mongo_exec`, `pgsql_params_to_config`, `mongo_params_to_config`, `build_tunnel_config_for_pgsql`, `build_tunnel_config_for_mongo`
- Modify: `src/mcp/server.rs` — add `#[tool] async fn pgsql_exec` + `#[tool] async fn mongo_exec` on the `ToolsMcpServer`

- [ ] **Step 1: Add MCP-side tool boilerplate**

The existing `redis_exec` setup is the closest reference — it has `RedisExecParams`, `redis_exec`, `redis_params_to_config`, `build_tunnel_config_for_redis`, plus the `#[tool]` method on the server. Read `src/mcp/tools.rs` for the existing `RedisExecParams` shape; clone it twice with the right ServiceType variant + the tool's name.

`PgsqlExecParams` field set is identical to `MysqlExecParams` (query + host/port/user/password/database/profile/config + tunnel/ssh_*). Use it as the template.

`MongoExecParams` field set: same shape but with `command` instead of `query`, and `database` is required at validation time (mongo's `from_config` enforces this). Mirror `RedisExecParams`'s top half but rename `command` and add `database`.

The `#[tool]` annotated method on `ToolsMcpServer` is mechanical — match the style of the existing `redis_exec` method (signature, `Parameters<X>` extractor, error→McpError mapping).

- [ ] **Step 2: Verify**

```
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Make sure the existing `tests/mcp_smoke.rs` still passes; if it asserts a specific tool list it'll need bumping to 6 tools.

- [ ] **Step 3: Update `mcp_smoke.rs` if it counts tools**

Read `tests/mcp_smoke.rs`. If it greps for `mysql_exec` / `redis_exec` / `http_exec` / `ssh_exec`, add `pgsql_exec` and `mongo_exec`.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(mcp): add pgsql_exec + mongo_exec MCP tools

Mirrors mysql_exec/redis_exec — same Profile/YAML/explicit-field merge,
same tunnel routing, same ExecutionResult output. Tools registered on
ToolsMcpServer via #[tool] macro.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Final verification + workspace cleanup

- [ ] **Step 1: Workspace-wide green**

```
cargo build
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release
```

- [ ] **Step 2: CLI smoke for all 6 services**

```bash
for s in mysql redis http ssh pgsql mongo; do
  echo "=== $s ==="
  ./target/release/tools-mcp $s --help | head -3
done
```

- [ ] **Step 3: MCP tool list smoke**

The existing `tests/mcp_smoke.rs` should (after Task 7 update) report 6 tools. Confirm via `cargo test --test mcp_smoke -- --nocapture`.

- [ ] **Step 4: Commit any final cleanup**

If anything was missed in earlier tasks (unused imports, stale doc comments mentioning "4 services" instead of 6, etc.), clean up in a single chore commit.

---

## Task 9: Documentation update

**Files:**
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md` (byte-equivalent edits except cross-link)
- Modify: `README.md`
- Modify: `skills/tools-mcp-using/SKILL.md` (add pgsql + mongo to the table + input shapes + error shapes + destructive-command list)

- [ ] **Step 1: CLAUDE.md / AGENTS.md**

- Bump phase reference from Phase 8 to Phase 9.
- Update the Module map: insert rows for `tools-mcp-pgsql` and `tools-mcp-mongo` lib crates (alongside mysql/redis/http/ssh).
- Update the orchestrator row to mention `PgsqlOrchestrator` + `MongoOrchestrator`.
- Add a Phase 9 entry to the Phase boundaries section.
- Six services mentioned everywhere instead of four.

- [ ] **Step 2: README.md**

- Bump "Phase 8 release" → "Phase 9 release".
- Add a "PostgreSQL" subsection under Usage (CLI examples).
- Add a "MongoDB" subsection (CLI examples; show JSON-doc syntax for `runCommand`).
- Update the workspace paragraph to list all 7 lib crates (core + 5 services + orchestrator).
- Update the plugin section's MCP tool list to include `pgsql_exec` + `mongo_exec`.

- [ ] **Step 3: tools-mcp-using skill**

- Update the table at the top to include pgsql + mongo rows.
- Add input-shape examples for both.
- Add output-mapping notes (pgsql is standard tabular like mysql; mongo is single `result` row with JSON-stringified Document).
- Extend destructive-command list: pgsql (DROP/TRUNCATE/DELETE-without-WHERE/REVOKE), mongo (`drop`, `dropDatabase`, `delete` with broad filter, `update` with `multi:true`, `findAndModify` with `remove:true`, `createUser`/`dropUser` admin commands).

- [ ] **Step 4: Verify + commit**

```bash
diff CLAUDE.md AGENTS.md  # only the cross-link lines should differ
cargo build  # docs change shouldn't break anything
```

```bash
git commit -am "docs: document Phase 9 PostgreSQL + MongoDB support

CLAUDE/AGENTS/README updated: phase bump, two new lib-crate rows in
the module map, two new orchestrator names, two new MCP tools, two
new CLI subcommands. Phase boundaries gains a Phase 9 entry.

tools-mcp-using skill extended with pgsql + mongo input shapes,
output mapping, and destructive-command list.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Summary

After Phase 9:

- **Six service lib crates** in `crates/`: core, mysql, pgsql (NEW), redis, mongo (NEW), http, ssh.
- **Six service orchestrators** in `tools-mcp-orchestrator`: Mysql / Pgsql / Redis / Mongo / Http / SshDirect — all impl `Service` trait.
- **Six CLI subcommands**: `mysql / pgsql / redis / mongo / http / ssh`.
- **Six MCP tools**: `mysql_exec / pgsql_exec / redis_exec / mongo_exec / http_exec / ssh_exec`.
- Profile/YAML 3-layer merge supported for the 4 typed-database services (mysql, pgsql, redis, mongo); HTTP and SSH-direct still take typed requests directly.
- Mongo command syntax: JSON document → `Database::run_command`. Result Document serialized to JSON in a single `result` row.
- Postgres type mapping: bool/int2-8/float4-8/text/varchar/bpchar/name/date/time/timestamp(tz)/json/jsonb. Other types render as `<typename>`.

**Deferred:**
- TLS for pgsql / mongo (NoTls / rustls-tls feature already enabled but not wired into the URL builder yet).
- Pgsql prepared statements / COPY / LISTEN/NOTIFY / typed parameter binding.
- Mongo cursor pagination beyond the first batch.
- uuid + cidr/inet types for postgres.
