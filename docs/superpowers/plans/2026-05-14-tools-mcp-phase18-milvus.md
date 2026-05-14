# Tools MCP Phase 18: Milvus leaf crate

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or executing-plans).
>
> **Prerequisites:** Phase 17 (rabbitmq leaf merged). Same leaf-crate shape.

**Goal:** Add a `tools4a-milvus` leaf crate that talks to a Milvus vector database via the BenLocal/milvus-sdk-rust fork (branch `self`). Ten MCP tools covering diagnosis (list/describe/stats/query), vector search, and lifecycle management (drop/load/release).

**SDK source**: `https://github.com/BenLocal/milvus-sdk-rust` branch `self`. This is a Cargo git dep, not crates.io. Pin via `git = "..."`, `branch = "self"` in `Cargo.toml`. The user picked this branch specifically; do not switch to upstream.

**Wire protocol**: Milvus is gRPC (tonic 0.11). Default port 19530. URI form: `http://host:port`. tools4a exposes `--host` + `--port` for ergonomics, builds URI internally.

**MCP tools to ship (10):**

| Tool | Action | R/W | SDK call |
|---|---|---|---|
| `milvus_list_databases` | List DBs | read | `Client::list_databases` |
| `milvus_list_collections` | List collections [in db] | read | `Client::list_collections` |
| `milvus_describe_collection` | Schema + fields + indexes | read | `Client::describe_collection` |
| `milvus_collection_stats` | row_count etc. | read | `Client::get_collection_stats` |
| `milvus_list_partitions` | Partitions of a collection | read | `Client::list_partitions` |
| `milvus_query` | Scalar filter query | read | `Client::query` |
| `milvus_search` | Vector ANN search | read | `Client::search` |
| `milvus_drop_collection` | Delete collection | **write** | `Client::drop_collection` |
| `milvus_load_collection` | Load into mem | **write** | `Client::load_collection` |
| `milvus_release_collection` | Release from mem | **write** | `Client::release_collection` |

Write actions require `allow_write=true` per Phase 10 pattern.

**Out of scope (deferred to a later phase):**
- `create_collection` — requires CollectionSchemaBuilder + field definitions; non-trivial to expose through JSON params well.
- Insert / upsert / delete data — bulk data path, format-heavy.
- Index management (create_index / drop_index / list_indexes).
- Alias management.
- Resource groups.

These can come in Phase 18b if needed.

---

## Architecture

Standard leaf-crate shape (per Phase 11):

- `connection.rs` — small `MilvusConnect` helper: builds a `milvus::Client` via `ClientBuilder::new(uri).username(...).password(...).timeout(...).build().await`. Auth + timeout + URI all live here.
- `actions.rs` — `MilvusAction` enum + 10 action functions. Each takes `&milvus::Client` + typed args, calls one SDK method, shapes the response into `ExecutionResult` rows.
- `run.rs` — single dispatcher.
- `orchestrator.rs` — `MilvusRequest` + `MilvusOrchestrator: impl Service`. `allow_write` gating. Builds tunnel via `build_tunnel`; for tunneled mode points the URI at the tunnel local endpoint (lesson from Phase 17's reqwest IP-literal bug).
- `mcp.rs` — 10 `McpTool` impls + shared `MilvusConnectionFields` (host/port/user/password/database/timeout + standard tunnel fields).

**Vector data input shape**: `milvus_search` accepts vectors as a JSON 2D array of floats: `[[0.1, 0.2, ...], [...]]`. Each inner array becomes a `Value::FloatArray`. Tools4a parses + converts. Distance metric (`L2` / `IP` / `COSINE`) is a separate `--metric` param.

**Query / Search result shape**: SDK returns `Vec<FieldColumn>` (column-oriented). We transpose to row-oriented `ExecutionResult` via `ValueVec::len()` for row count + per-cell stringification (`Bool` / `Int` / `Long` / `Float` / `Double` / `String` / `Json` -> string; `Binary` / `Bytes` / `Geometry` / `Array` -> placeholder).

---

## Tunnel handling

Same lesson as Phase 17: when going through `SshTunnel`, point the URI directly at `http://127.0.0.1:<tunnel-port>` rather than using any DNS resolve trick. tonic's gRPC client probably has the same IP-literal limitation, and even if not, the simpler "URI follows tunnel" approach is robust. TLS through tunnel deferred to Phase 18b.

---

## File Structure

**New crate:**
- `crates/tools4a-milvus/Cargo.toml`
- `crates/tools4a-milvus/src/lib.rs`
- `crates/tools4a-milvus/src/connection.rs`
- `crates/tools4a-milvus/src/actions.rs`
- `crates/tools4a-milvus/src/run.rs`
- `crates/tools4a-milvus/src/orchestrator.rs`
- `crates/tools4a-milvus/src/mcp.rs`

**Modified (bin wiring, mirrors Phase 15/17):**
- `Cargo.toml` workspace member + bin dep.
- `src/cli/args.rs` — `Milvus` variant + `MilvusCommand` enum with 10 subcommands.
- `src/cli/handler.rs` — `execute_milvus` dispatcher.
- `src/cli/mod.rs` — re-export `MilvusCommand`.
- `src/mcp/server.rs` — 10 `#[tool]` methods.

**Cargo.toml git dep shape:**

```toml
[dependencies]
milvus-sdk-rust = { git = "https://github.com/BenLocal/milvus-sdk-rust.git", branch = "self" }
```

Note: this pulls tonic 0.11, prost 0.12, base64 0.21, dashmap — large dep graph and slow first build. Subsequent builds are cached.

---

## Tasks

1. Plan file.
2. Crate skeleton (Cargo.toml + lib.rs) + `connection.rs`.
3. `actions.rs` (10 actions) + `run.rs`.
4. `orchestrator.rs` + `mcp.rs`.
5. Bin wiring.
6. `make ci` green.

Estimated total: ~1200-1500 lines new + ~120 lines bin wiring.

---

## Commit style

One commit:

```
feat(milvus): add tools4a-milvus leaf crate (Phase 18)

New leaf crate wrapping the BenLocal/milvus-sdk-rust fork (branch
"self") for the Milvus vector database. Ten MCP tools:

- 6 read: list_databases / list_collections / describe_collection /
  collection_stats / list_partitions / query
- 1 vector: search (ANN with output fields + metric + limit)
- 3 write (allow_write gated): drop_collection / load_collection /
  release_collection

Standard tunnel routing via SshTunnel; URI points at the tunnel local
endpoint when tunneled (same lesson from Phase 17 — gRPC client likely
has the same IP-literal resolve limitation as reqwest, and the
URI-follows-tunnel approach is robust regardless).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```
