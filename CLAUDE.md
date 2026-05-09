# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本文件与同目录的 [`AGENTS.md`](./AGENTS.md) 内容保持一致。修改任意一份时，请同步另一份。`AGENTS.md` 是给非 Claude 的 AI 编辑器（Cursor / Copilot / Codex 等）使用的等价文档。

## Project Overview

`tools4a` is a Rust CLI + MCP server for MySQL, PostgreSQL, Redis, MongoDB, HTTP, and SSH. **Phase 11 (current) flattens the architecture: `tools4a-orchestrator` is dissolved and each leaf crate (`tools4a-mysql`, `tools4a-pgsql`, `tools4a-redis`, `tools4a-mongo`, `tools4a-http`, `tools4a-ssh`) now owns its full vertical slice — protocol primitives + `<Svc>Orchestrator` (impl `core::Service`) + `<Svc>Mcp` (impl `core::McpTool`).** Tunnel runtime impls live in a new `tools4a-tunnel` crate; `Config` / `Profile` / `ConfigLoader` / `ConfigMerger` and the `McpTool` trait moved to `tools4a-core`. All six services ship as CLI subcommands (`mysql`, `pgsql`, `redis`, `mongo`, `http`, `ssh`) and MCP tools (`mysql_exec`, `pgsql_exec`, `redis_exec`, `mongo_exec`, `http_exec`, `ssh_exec`). Profile/YAML 3-layer merge supported by mysql, pgsql, redis, mongo (not http or ssh-direct). The design spec lives at `docs/superpowers/specs/2026-05-07-tools4a-design.md`; recent plans are in `docs/superpowers/plans/`.

## Common Commands

The `Makefile` wraps cargo:

- `make` / `make help` — list targets
- `make build` / `make release` — debug / release build
- `make test` — full suite (unit + integration)
- `make clippy` — clippy with `-D warnings`
- `make fmt` / `make fmt-check` — rustfmt
- `make ci` — `fmt-check` → `clippy` → `test`
- `make run ARGS="..."` — run debug binary; **prefer `cargo run -- ...` directly when args contain `!` or `#`** because bash history expansion mangles `ARGS="..."` before make sees it.

Single test: `cargo test <test_name>` (e.g. `cargo test test_load_toml_config`).
Single integration test crate: `cargo test --test config_tests`.

## Architecture

**Workspace.** The repo is a Cargo workspace. The `tools4a` binary crate lives at the repo root (presentation layer only: `cli/`, `mcp/`, `output/`, `main.rs`, `lib.rs`). The lib crates under `crates/` are: `tools4a-core` (trait floor + `TunnelConfig` + `Service` + `McpTool` traits + `Config` / `Profile` / `Loader` / `Merger` + `readonly` helpers), `tools4a-tunnel` (`DirectTunnel` + `SshTunnel` impls + `build_tunnel` helper), and the six leaf service crates (`tools4a-mysql`, `tools4a-pgsql`, `tools4a-redis`, `tools4a-mongo`, `tools4a-http`, `tools4a-ssh`). Each leaf crate owns its full vertical slice: `connection.rs`/`executor.rs`/`execute.rs` (protocol primitives), `orchestrator.rs` (`<Svc>Request` + `<Svc>Orchestrator: impl Service`), and `mcp.rs` (`<Svc>ExecParams` + `<Svc>Mcp: impl McpTool`). Adding a new service is one new leaf crate with those three modules plus a leaf-crate dep added in the bin's `Cargo.toml` and four lines in `src/cli/handler.rs` + `src/mcp/server.rs`.

### Module map (each module = one responsibility)

| Crate / Module | Role |
| --- | --- |
| `tools4a-core` (lib) | `Tunnel` / `Connection` / `Service` / `McpTool` async traits; `TunnelEndpoint`, `TunnelConfig`, `Error` / `Result`, `ExecutionResult`; `config::{Config, Profile, ConfigLoader, ConfigMerger, ServiceType}` (3-layer merge primitives); `mcp::{TunnelKind, SshJumpInput, build_tunnel_config}` (shared MCP types); `readonly::{is_readonly_sql, is_readonly_mongo}` (write-gating helpers). Deps: `async-trait` + `serde` + `serde_json` + `serde_yml` + `toml` + `schemars`. The dependency floor for the workspace. |
| `tools4a-tunnel` (lib) | `DirectTunnel` + `SshTunnel` runtime impls (impl `core::Tunnel`) + `build_tunnel(host, port, Option<TunnelConfig>) -> Box<dyn Tunnel>` helper. Each leaf crate's `<Svc>Orchestrator::execute` calls `build_tunnel` to produce the right tunnel before dispatching to `<lib>::execute`. Deps: `russh`, `tokio`, `tools4a-core`, `tools4a-ssh` (for `build_session_chain`). |
| `tools4a-mysql` (lib) | Protocol primitives (`MySQLConnection`, `MySQLExecutor`, `execute(tunnel, params, query, read_only) -> ExecutionResult`); `MysqlOrchestrator` + `MysqlRequest` (impl `Service`, with `allow_write` field + readonly gating); `MysqlMcp` + `MysqlExecParams` (impl `McpTool`, name `"mysql_exec"`, does the 3-layer config merge then dispatches through `MysqlOrchestrator`). Owns `mysql_async`. |
| `tools4a-pgsql` (lib) | Same shape as mysql. Type mapping: bool / int2-8 / float4-8 / text / varchar / bpchar / name / date / time / timestamp(tz); other types render as `<typename>`. Owns `tokio-postgres`. |
| `tools4a-redis` (lib) | Same shape as mysql but no `allow_write` (Redis is shell-shaped — opt-out from write gating). Owns `redis` (with `tokio-comp`) + `shlex`. |
| `tools4a-mongo` (lib) | Same shape as mysql; readonly gating uses `is_readonly_mongo` (whitelist of read commands; aggregate inspects pipeline for `$out`/`$merge`). Owns the official `mongodb` 3.x driver. |
| `tools4a-http` (lib) | Protocol primitives (`HttpRequestSpec`, `HttpAuth`, `HttpExecutor`, `execute`); `HttpOrchestrator` (uses `HttpRequestSpec` directly — no `from_config` since HTTP defers Profile/YAML); `HttpMcp` + `HttpExecParams` (impl `McpTool`). Owns `reqwest 0.12` (rustls-tls + gzip + brotli + stream). |
| `tools4a-ssh` (lib) | `AcceptAnyHostKey` / `authenticate` / `build_session_chain` (shared with `tools4a-tunnel`'s `SshTunnel`); `SshExecRequest`; `SshDirectOrchestrator`; `SshMcp` + `SshExecParams`. Owns the russh `session` channel + `exec` request glue. |
| `tools4a` bin (`src/cli/*`) | clap `Cli`, `SshTunnelArgs`, `CliHandler` — CLI mode parse + dispatch. Builds typed `<Svc>Request` (via `<Svc>Request::from_config` for mysql/pgsql/redis/mongo, directly for http/ssh) and calls `<Svc>Orchestrator::execute(req, tunnel)`. |
| `tools4a` bin (`src/mcp/server.rs`) | rmcp `ServerHandler`. Each `<svc>_exec` `#[tool]` method is a one-liner: `into_call_result(<Svc>Mcp::invoke(params).await)`. No per-service params/dispatch logic in the bin — it all lives in the leaf crates. |
| `tools4a` bin (`src/output/cli.rs`) | `CliFormatter` — comfy-table renderer. Operates on `tools4a_core::ExecutionResult`. |

### Config priority (low → high)

1. `[profiles.<NAME>]` from `~/.config/tools4a/config.toml` when `--profile <NAME>` is set
2. YAML file when `--config <PATH>` is set
3. CLI args

Each layer is a `Config`; `ConfigMerger::merge_multiple` folds them so later layers win per-field.

### Phase boundaries (where to gate features that aren't yet implemented)

- **SSH tunnel**: implemented in Phase 2 via `tunnel::SshTunnel` (russh-based). Single- and multi-hop jumps via comma-separated `--ssh-jump`; password or key auth; host keys accepted with stderr fingerprint warning. Strict known_hosts verification, key passphrases, and per-hop auth are Phase 3.
- **MCP server mode**: implemented in Phase 3. `main.rs` runs `mcp::serve_stdio` when no subcommand is given. Single tool `mysql_exec` (in `mcp::tools`) routes to `core::mysql::execute` — same execution path as `tools4a mysql "..."`.
- **Redis subcommand**: implemented in Phase 5. `tools4a redis "..."` and the `redis_exec` MCP tool both route through `core::redis::execute`. The `db` field is on `Config`/`Profile` for the Redis database number.
- **HTTP subcommand**: implemented in Phase 6. `tools4a http <METHOD> <URL>` and the `http_exec` MCP tool both route through `core::http::execute`. Tunnel routing uses reqwest's `resolve(host, addr)` override so HTTPS through SSH tunnels preserves SNI / Host header / cert verification. Phase 6 deliberately doesn't support Profile/YAML for HTTP — only CLI flags + global tunnel.
- **SSH-direct subcommand**: implemented in Phase 7. `tools4a ssh "<COMMAND>"` and the `ssh_exec` MCP tool both route through `core::ssh::execute`. TARGET credentials are separate from JUMP credentials (when `--tunnel=ssh` is used) — that's by design (Model A). Reuses the Phase 2 multi-hop infrastructure via `tools4a_ssh::session::build_session_chain` (extracted from the bin in Phase 7 so both `SshTunnel` and `SshExec` share it). Phase 7 deliberately doesn't support Profile/YAML — only CLI flags + global tunnel.
- **Service trait + orchestrator crate split**: implemented in Phase 8. `Service` trait lives in `tools4a-core`; the four orchestrators (`MysqlOrchestrator`, `RedisOrchestrator`, `HttpOrchestrator`, `SshDirectOrchestrator`) live in the new `tools4a-orchestrator` crate, each implementing the trait. MySQL and Redis orchestrators have a typed `<Svc>Request::from_config(config, action) -> Result<Self>` constructor that consolidates host/user/etc. validation; HTTP and SSH-direct skip the `from_config` ctor because they don't take `Config` (Profile/YAML deferred). The bin's `src/{config,tunnel,core}/` directories are deleted; CLI and MCP layers build typed requests and dispatch via `<Svc>Orchestrator::execute(req, tunnel)`.
- **PostgreSQL + MongoDB support**: implemented in Phase 9. Two new lib crates (`tools4a-pgsql` wrapping `tokio-postgres`, `tools4a-mongo` wrapping the official `mongodb` 3.x driver) plus matching `PgsqlOrchestrator` / `MongoOrchestrator` (both impl `Service` with their own typed Request + `from_config` constructor). CLI gains `tools4a pgsql "..."` and `tools4a mongo "..."`; MCP gains `pgsql_exec` / `mongo_exec`. Profile/YAML 3-layer merge supported (pgsql + mongo are typed-database services like mysql/redis). Mongo command syntax: JSON document passed to `Database::run_command`; result Document serialized to JSON in a single `result` row. Pgsql type mapping: bool / int2-8 / float4-8 / text / varchar / bpchar / name / date / time / timestamp(tz); other types render as `<typename>` placeholders.
- **Read-only by default for mysql / pgsql / mongo**: implemented in Phase 10. `MysqlRequest` / `PgsqlRequest` / `MongoRequest` gained an `allow_write: bool` field (defaults to false in `from_config`). `MysqlOrchestrator` / `PgsqlOrchestrator` / `MongoOrchestrator::execute` reject non-read queries before connecting via `tools4a_core::readonly::{is_readonly_sql, is_readonly_mongo}` — first-keyword whitelist for SQL (`SELECT`/`SHOW`/`EXPLAIN`/`DESCRIBE`/`WITH`/`VALUES`/`TABLE`/`USE`); curated command whitelist for Mongo (`find`/`aggregate` without `$out`/`$merge`/`count`/`distinct`/`list*`/etc.). For SQL, the lib crate's `execute(...)` also takes a `read_only: bool` and runs `SET SESSION TRANSACTION READ ONLY` (MySQL) / `SET default_transaction_read_only = on` (Postgres) as belt-and-suspenders. Mongo has no per-session read-only mode, so the orchestrator-level whitelist is the only guard. CLI flag is `--allow-write`; MCP field is `allow_write` (default false) on `MysqlExecParams` / `PgsqlExecParams` / `MongoExecParams`. **redis, http, and ssh are NOT gated** — they accept any command without `allow_write` (Redis is shell-shaped and HTTP/SSH already encode write semantics in their method/command). Phase 10 also fixed a latent bug in `tools4a-pgsql::execute` and `tools4a-mongo::execute` where `connect()` was never called — they now call it before running.
- **`McpTool` trait + per-leaf MCP impls**: implemented in Phase 11. `tools4a-orchestrator` is dissolved; each leaf crate now owns its full vertical slice. `tools4a-core` gains the `McpTool` trait (`NAME` / `DESCRIPTION` consts + `Params` type + async `invoke`), the shared `TunnelKind` / `SshJumpInput` / `build_tunnel_config` MCP helpers, and `Config` / `Profile` / `ConfigLoader` / `ConfigMerger` (moved from orchestrator). `DirectTunnel` + `SshTunnel` impls move to a new `tools4a-tunnel` crate that exposes `build_tunnel(host, port, Option<TunnelConfig>) -> Box<dyn Tunnel>`. Each leaf crate (`tools4a-{mysql,pgsql,redis,mongo,http,ssh}`) gains `orchestrator.rs` (`<Svc>Request` + `<Svc>Orchestrator: impl Service`) and `mcp.rs` (`<Svc>ExecParams` + `<Svc>Mcp: impl McpTool`). The bin's `src/mcp/tools.rs` is deleted; `src/mcp/server.rs` becomes thin — each `#[tool]` method is `into_call_result(<Svc>Mcp::invoke(params).await)`. New service = new leaf crate with three modules + four lines in the bin.

## Conventions worth knowing

- **Cargo edition is `2024`** (not `2021` as the Phase 1 plan text says). Don't "fix" it back.
- **YAML crate is `serde_yml`** (the maintained fork). `serde_yaml` was deprecated upstream and removed during Phase 1 cleanup.
- **`lib.rs` declares only modules whose files exist.** When adding a new module file, also add its `pub mod X;` line. The Phase 1 plan listed all 7 modules upfront — that pattern fails to compile.
- **`--tunnel` is a flag, not a subcommand.** The original plan had a second `#[command(subcommand)]` alongside `command`, which clap rejects. Correct shape: `--tunnel direct|ssh` (`ValueEnum`) + flat `--ssh-*` global flags inside `SshTunnelArgs`. Match this when adding more tunnel kinds.
- **Help layout**: `Usage: tools4a [GLOBAL OPTIONS] <cmd> [OPTIONS] ...` uses `[GLOBAL OPTIONS]` as a handwritten placeholder; the constant `USAGE_LEGEND` in `cli/args.rs` (rendered via `after_help` on root and each subcommand) maps placeholders to actual help-section names. Adding a global flag is just `global = true` + `help_heading`; the `override_usage` strings don't need updating.
- **Path arguments use `PathBuf`**, not `String` (e.g. `--config`).
- **`Error::source()`**: lives in `tools4a-core`. The wrapping variant for non-IO errors is `Error::Service(String)` — service-specific error types (`mysql_async::Error`, `russh::Error`, `serde_yml::Error`, `toml::de::Error`) are flattened to a string at the boundary so core stays dep-free. If a future service needs to expose typed inner errors through `source()`, prefer adding a typed variant to `tools4a-<service>::Error` and only flattening at the bin boundary.
- **`main.rs` prints errors via `Display` to stderr** (not `Debug`). If touching `main()`, keep the explicit `if let Err(e) = ... { eprintln!("Error: {e}"); exit(1); }` pattern.
- **Stray SSH flags with `--tunnel=direct`** are a runtime `Error::Config` (not silently ignored); see `cli_to_tunnel_config`.
- **CLI <-> MCP parity**: every CLI subcommand has (or will have) a paired MCP tool, and both delegate to the same `core::<service>` function. When adding a new subcommand, write the core function first, then wire CLI and MCP on top — never embed business logic in either presentation layer.
- **Service-specific Profile/YAML support is opt-in**: MySQL, PostgreSQL, Redis, and MongoDB use the 3-layer merge (TOML profile → YAML → CLI args); HTTP and SSH-direct currently don't (Phase 6/7 simplification). When adding a new service, decide upfront whether profile support is in scope; if yes, add the relevant fields to `Profile` and `Config` and a `build_config_<svc>` sibling in `cli/handler.rs`. If no, follow the HTTP/SSH pattern — orchestrator takes a typed request struct + `Option<TunnelConfig>` directly, no `Config` plumbing.
- **Two-credential separation for ssh-direct**: when SSH is BOTH the tunnel transport AND the target service (i.e. `tools4a ssh ... --tunnel=ssh`), the JUMP credentials (`ssh_user`/`ssh_password`/`ssh_key_path`) and the TARGET credentials (`user`/`password`/`key_path`) are independent. Don't infer one from the other. The session chain authenticates with jump creds; the final SSH session (over the last jump's direct-tcpip channel) authenticates with target creds.
- **Per-service config fields**: `Config` is a flat bag of all possible fields across services (`database` for MySQL, `db` for Redis, `key_path` for SSH, etc.). Each orchestrator picks out only what it needs. When adding a new service that requires a new field, add it to `Profile` and `Config` (and the `ConfigMerger::merge` `or` chain) — but only if existing fields can't carry the meaning.
- **Service trait + typed Request pattern**: every leaf crate's `<Svc>Orchestrator` implements `tools4a_core::Service` with `type Request = <Svc>Request` (or the existing typed input from the service lib, e.g. `HttpRequestSpec` / `SshExecRequest`). The unified signature is `async fn execute(Self::Request, Option<TunnelConfig>) -> Result<ExecutionResult>`. CLI and MCP layers MUST go through this — they construct the typed Request (validating + draining `Config` for mysql/pgsql/redis/mongo, building directly from flags/JSON for http/ssh) then dispatch. Never bypass the Service impl with a free function.
- **McpTool trait pattern**: every leaf crate's `<Svc>Mcp` implements `tools4a_core::McpTool` with `NAME` / `DESCRIPTION` consts and a `Params` type that derives `Deserialize` + `JsonSchema`. The async `invoke(params)` does the 3-layer config merge (where applicable) and dispatches through `<Svc>Orchestrator::execute`. The bin's `src/mcp/server.rs` is the ONLY place that touches rmcp; per-service plumbing lives in the leaf crate. Adding a new MCP tool = add `<Svc>Mcp` to the leaf + one `#[tool]` method in `server.rs`.

## Implementation methodology

Phase 1 was built with `superpowers:subagent-driven-development` against the written plan: each task is one focused commit using TDD (failing test → impl → green), followed by spec-compliance + code-quality review. When extending Phase 1 or starting Phase 2:

- Write a plan in `docs/superpowers/plans/` before touching code
- Tests come before implementation (`cargo test <name>` should fail until the impl exists)
- One commit per task, with the `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailer used in existing commits
