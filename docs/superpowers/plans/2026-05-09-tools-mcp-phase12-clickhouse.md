# Tools MCP Phase 12: ClickHouse Support

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to execute this plan task-by-task.

**Goal:** Add a `tools4a-clickhouse` lib crate modeled on `tools4a-pgsql`, plus matching `ClickhouseOrchestrator` (impl `Service` via typed `ClickhouseRequest::from_config`), CLI subcommand `tools4a clickhouse "..."`, and MCP tool `clickhouse_exec`. Fits the Phase 11 architecture exactly — every new piece mirrors the pgsql/mysql vertical-slice pattern.

**Architecture:**
- ClickHouse uses `clickhouse = "0.15"` with `default-features = false` (the official driver). Talks HTTP/1.1 to ClickHouse's HTTP interface (port 8123). HTTPS is **out of scope** for v1 — see "Out of scope" below.
- For arbitrary ad-hoc SQL with dynamic result schemas, we use `Query::fetch_bytes("JSONCompact").collect()` to get the raw response bytes, then parse JSON into `ExecutionResult`. The crate's typed `Row` API doesn't fit a CLI/MCP that gets arbitrary user SQL.
- ClickHouse uses standard SQL keywords for read-only classification — `is_readonly_sql` from `tools4a-core` covers `SELECT` / `SHOW` / `EXPLAIN` / `DESCRIBE` / `WITH` cleanly. As belt-and-suspenders, we also set `readonly=1` via `Client::with_setting` when `allow_write` is false.
- Profile/YAML 3-layer merge supported (typed-database service like mysql/pgsql/redis/mongo), so `ServiceType` gains a `Clickhouse` variant.

**Tech Stack:**
- New deps: `clickhouse = { version = "0.15", default-features = false }`, `serde_json = "1.0"`.
- Existing deps reused: `async-trait`, `serde`, `tokio`, `schemars`, `tools4a-core`.

**Out of scope (future phases):**
- **HTTPS** to ClickHouse server. The `clickhouse` crate uses hyper directly; for HTTPS-via-SSH-tunnel we'd need the same `resolve(host, addr)` SNI-preservation trick that `tools4a-http` does, but the official crate doesn't expose reqwest's `resolve` API. We'd need to inject a custom `HttpClient` impl. Defer until someone actually needs it.
- Compression (`lz4` / `zstd` features). Default features disabled; can be flipped on later.
- ClickHouse Cloud JWT auth (`with_access_token`). User/password only for v1.
- `INSERT` flow via `client.insert::<T>()` typed API — writes still go through `query(sql).execute()` which works for `INSERT ... VALUES (...)` literal SQL.

---

## File Structure

```
crates/
└── tools4a-clickhouse/                          (NEW)
    ├── Cargo.toml
    └── src/
        ├── lib.rs                                pub mod + re-exports
        ├── connection.rs                         ClickhouseConnection (impl core::Connection)
        ├── executor.rs                           ClickhouseExecutor (JSONCompact → ExecutionResult)
        ├── execute.rs                            ClickhouseParams + execute(tunnel, params, sql, read_only)
        ├── orchestrator.rs                       ClickhouseRequest + ClickhouseOrchestrator (impl Service)
        └── mcp.rs                                ClickhouseExecParams + ClickhouseMcp (impl McpTool)
```

In `tools4a-core`:

```
crates/tools4a-core/src/config/types.rs          add ServiceType::Clickhouse + FromStr aliases
```

In bin:

```
Cargo.toml                                        + tools4a-clickhouse path dep + workspace member
src/cli/args.rs                                   + Commands::Clickhouse
src/cli/handler.rs                                + Commands::Clickhouse arm + execute_clickhouse
src/mcp/server.rs                                 + #[tool] clickhouse_exec + import + ServerInfo blurb
```

---

## Task 1: `ServiceType::Clickhouse` in core

- Add `Clickhouse` variant to `ServiceType` in `crates/tools4a-core/src/config/types.rs`.
- Add `"clickhouse"` and `"ch"` aliases in `FromStr`.
- Add a unit test for both aliases.

## Task 2: Scaffold `tools4a-clickhouse` crate

- Create `crates/tools4a-clickhouse/Cargo.toml` with deps listed above.
- Create `src/lib.rs` declaring `pub mod {connection, execute, executor, mcp, orchestrator};` plus re-exports of the public surface.
- Add to root `Cargo.toml` `[workspace] members` (between `mongo` and `mysql` alphabetically — wait, `clickhouse` sorts before all, so first in the list).
- Add `tools4a-clickhouse = { path = "crates/tools4a-clickhouse" }` to the bin's `[dependencies]`.
- `cargo check -p tools4a-clickhouse` should compile (empty modules ok).

## Task 3: `connection.rs`

- `ClickhouseConnection { tunnel, user, password, database, allow_write, client }` with `client: Option<clickhouse::Client>`.
- `new(...)` constructor + `client()` accessor returning `&Client` or `Error::Connection`.
- `Connection::connect`: `tunnel.establish()` → build `clickhouse::Client::default().with_url(format!("http://{host}:{port}"))` + creds + `with_database(db)` (only if Some) + `with_setting("readonly","1")` (only if `!allow_write`). Stash in `self.client`.
- `Connection::disconnect`: drop client + `tunnel.close()`.
- Unit test for `new` only (no live ClickHouse needed).

## Task 4: `executor.rs`

- `ClickhouseExecutor::execute(&conn, query) -> ExecutionResult`. Internally:
  1. `conn.client()?.query(sql).fetch_bytes("JSONCompact")?.collect().await?` → `Bytes`
  2. Parse with `serde_json::from_slice` into a local `JsonCompactResponse { meta: Vec<MetaCol{name, type}>, data: Vec<Vec<Value>> }`.
  3. `columns` = `meta.iter().map(|c| c.name).collect()`.
  4. Each row: stringify each `Value` with our local `value_to_string` (numbers/bools/strings rendered raw, null → `"NULL"`, objects/arrays → `serde_json::to_string`).
  5. `affected_rows` = row count (ClickHouse JSONCompact doesn't expose write affected counts; v1 returns row count).
- Unit tests with hardcoded `JSONCompact` JSON fixtures (parse-only, no live server).

## Task 5: `execute.rs`

- `ClickhouseParams { user, password, database, allow_write }` (need `allow_write` here to thread through to `ClickhouseConnection::new`).
- `pub async fn execute(tunnel, params, query, read_only) -> Result<ExecutionResult>`:
  - `read_only` param from caller is independent of `params.allow_write` — both belt and suspenders. Caller passes `!allow_write` for read_only.
  - Build connection, `connect().await?`, run `ClickhouseExecutor::execute`, then `disconnect().await` (best-effort drop).
- Note: `read_only` is passed through to the `Connection::connect` impl via `ClickhouseConnection.allow_write` field — `connect()` adds `with_setting("readonly","1")` when `!allow_write`.

## Task 6: `orchestrator.rs`

- `ClickhouseRequest { host, port, user, password, database, query, allow_write }`.
- `from_config(config: Config, query: String) -> Result<Self>`:
  - host: required (`Error::Config("Clickhouse host is required")`).
  - port: default 8123.
  - user: default `"default"` (ClickHouse's default username when none configured).
  - password / database: optional pass-through.
  - allow_write: defaults false.
- `ClickhouseOrchestrator: Service`:
  - Reject non-readonly query without `allow_write` via `is_readonly_sql`.
  - `build_tunnel(req.host, req.port, tunnel_config)` → `execute::execute(tunnel, params, &req.query, !req.allow_write)`.
- Unit tests mirroring `pgsql/orchestrator.rs`: `from_config` validation + write-rejection.

## Task 7: `mcp.rs`

- `ClickhouseExecParams` mirrors `PgsqlExecParams` exactly (same fields: query, allow_write, host, port, user, password, database, profile, config, tunnel, ssh_*).
- `ClickhouseMcp: McpTool` with `NAME = "clickhouse_exec"`, description matching the CLI subcommand.
- `params_to_config` does the 3-layer merge (Profile → YAML → params) using `ConfigMerger`.
- `invoke`: extract `allow_write` + `query`, build config, build `ClickhouseRequest::from_config`, set `allow_write`, dispatch via `ClickhouseOrchestrator::execute`.

## Task 8: Wire bin

- `src/cli/args.rs`: add `Commands::Clickhouse { query, host, port, user, password, database, profile, allow_write }` (mirror Pgsql); port help says "default 8123".
- `src/cli/handler.rs`: add `Some(Commands::Clickhouse {...}) => ...` arm and `execute_clickhouse(...)` mirroring `execute_pgsql`. Reuse `build_config(...)` (already takes `ServiceType`).
- `src/mcp/server.rs`: import `tools4a_clickhouse::{ClickhouseExecParams, ClickhouseMcp}`. Add `#[tool]` `clickhouse_exec` method. Update `ServerInfo` instructions string to mention ClickHouse.
- Update root `Cargo.toml` MCP server instructions string in `src/mcp/server.rs::get_info` to include "ClickHouse".

## Task 9: CI pass

- `make ci` (`fmt-check` + `clippy -D warnings` + `test`) clean.
- Manual `cargo run -- clickhouse --help` smoke check shows the new subcommand.

---

## Conventions

- Every commit ends with the `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailer (matching existing commits).
- One commit per task above (or merge tasks 3–7 into one if the diff stays focused — the leaf crate is a single coherent unit).
- TDD where the test cost is low (`from_config`, JSONCompact parsing). Skip live-server integration tests — they need a running CH instance and don't exist for the other DBs in this repo either.
