# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本文件与同目录的 [`AGENTS.md`](./AGENTS.md) 内容保持一致。修改任意一份时，请同步另一份。`AGENTS.md` 是给非 Claude 的 AI 编辑器（Cursor / Copilot / Codex 等）使用的等价文档。

## Project Overview

`tools-mcp` is a Rust CLI + MCP server for MySQL, PostgreSQL, Redis, MongoDB, HTTP, and SSH. **Phase 9 (current) adds PostgreSQL and MongoDB support, bringing the total to six services. Two new lib crates (`tools-mcp-pgsql` wrapping `tokio-postgres`, `tools-mcp-mongo` wrapping the official `mongodb` 3.x driver) join the workspace alongside matching `PgsqlOrchestrator` and `MongoOrchestrator` in `tools-mcp-orchestrator` — both implementing `core::Service` with typed Request + `from_config` constructors.** All six services ship as CLI subcommands (`mysql`, `pgsql`, `redis`, `mongo`, `http`, `ssh`) and MCP tools (`mysql_exec`, `pgsql_exec`, `redis_exec`, `mongo_exec`, `http_exec`, `ssh_exec`). Profile/YAML 3-layer merge supported by mysql, pgsql, redis, mongo (not http or ssh-direct). The design spec lives at `docs/superpowers/specs/2026-05-07-tools-mcp-design.md`; recent plans are in `docs/superpowers/plans/`.

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

**Workspace.** The repo is a Cargo workspace. The `tools-mcp` binary crate lives at the repo root (presentation layer only: `cli/`, `mcp/`, `output/`, `connection/`, `main.rs`, `lib.rs`). The lib crates under `crates/` are: `tools-mcp-core` (trait floor + `TunnelConfig` + `Service` trait), `tools-mcp-mysql`, `tools-mcp-pgsql`, `tools-mcp-redis`, `tools-mcp-mongo`, `tools-mcp-http`, `tools-mcp-ssh` (each owns one service's primitives), and `tools-mcp-orchestrator` (owns the `Config` / `Profile` / `Loader` / `Merger` types, `DirectTunnel` / `SshTunnel` runtime impls, and the six `<Svc>Orchestrator: impl Service` types). Adding a new service typically means a new `tools-mcp-<svc>` lib crate plus a `<Svc>Orchestrator` in `tools-mcp-orchestrator`.

### Module map (each module = one responsibility)

| Crate / Module | Role |
| --- | --- |
| `tools-mcp-core` (lib) | `Tunnel` / `Connection` async traits, `TunnelEndpoint`, `TunnelConfig` (config-shape enum), `Service` trait (unified `execute(Self::Request, Option<TunnelConfig>) -> Result<ExecutionResult>`), `Error`/`Result`, `ExecutionResult`. Sole external deps: `async-trait` + `serde`. The dependency floor for the workspace. |
| `tools-mcp-mysql` (lib) | `MySQLConnection` (impl `core::Connection`), `MySQLExecutor`, and the entry `execute(tunnel, params, query) -> ExecutionResult`. Owns `mysql_async`. |
| `tools-mcp-pgsql` (lib) | `PgsqlConnection`, `PgsqlExecutor`, `execute(tunnel, params, query) -> ExecutionResult`. Owns `tokio-postgres`. Type mapping: bool / int2-8 / float4-8 / text / varchar / bpchar / name / date / time / timestamp(tz); other types render as `<typename>`. |
| `tools-mcp-redis` (lib) | `RedisConnection`, `RedisExecutor`, `execute(tunnel, params, command) -> ExecutionResult`. Maps `redis::Value` → `ExecutionResult`. Owns `redis` (with `tokio-comp`) + `shlex`. |
| `tools-mcp-mongo` (lib) | `execute(tunnel, params, command_json) -> ExecutionResult`. Parses the JSON command string → BSON Document → `Database::run_command`; serializes the result Document back to JSON in a single `result` row. Owns the official `mongodb` 3.x driver. |
| `tools-mcp-http` (lib) | `HttpRequestSpec`, `HttpExecutor`, `execute(tunnel, host, port, req) -> ExecutionResult`. Owns `reqwest 0.12` (rustls-tls + gzip + brotli + stream). Maps responses to flat `field`/`value` rows. |
| `tools-mcp-ssh` (lib) | `AcceptAnyHostKey` / `authenticate` / `build_session_chain` (shared across `SshTunnel` and `SshExec`); `SshExecRequest`; `execute(req, jumps)` entry. Owns the russh `session` channel + `exec` request glue. |
| `tools-mcp-orchestrator` (lib) | `Config` / `Profile` / `ConfigLoader` / `ConfigMerger` (3-layer merge primitives); `DirectTunnel` / `SshTunnel` runtime impls (impl `core::Tunnel`); `MysqlOrchestrator` + `MysqlRequest`; `PgsqlOrchestrator` + `PgsqlRequest`; `RedisOrchestrator` + `RedisRequest`; `MongoOrchestrator` + `MongoRequest` (all four with `from_config` ctors); `HttpOrchestrator` (uses `tools_mcp_http::HttpRequestSpec` directly — no `from_config` since HTTP defers Profile/YAML); `SshDirectOrchestrator` (uses `tools_mcp_ssh::SshExecRequest` directly — same reasoning). Each `<Svc>Orchestrator` impls `core::Service`. Deps: `async-trait` + `serde` + `serde_yml` + `toml` + all 6 service libs + `russh` + `reqwest` (URL parsing only) + `tokio`. |
| `tools-mcp` bin (`src/cli/*`) | clap `Cli`, `SshTunnelArgs`, `CliHandler` — CLI mode parse + dispatch. Builds typed `<Svc>Request` (via `<Svc>Request::from_config` for mysql/pgsql/redis/mongo, directly for http/ssh) and calls `<Svc>Orchestrator::execute(req, tunnel)`. |
| `tools-mcp` bin (`src/mcp/*`) | rmcp `ServerHandler`, six `<svc>_exec` tools — same flow as CLI handler, just with JSON params instead of clap structs. |
| `tools-mcp` bin (`src/output/cli.rs`) | `CliFormatter` — comfy-table renderer. Operates on `tools_mcp_core::ExecutionResult`. |

### Config priority (low → high)

1. `[profiles.<NAME>]` from `~/.config/tools-mcp/config.toml` when `--profile <NAME>` is set
2. YAML file when `--config <PATH>` is set
3. CLI args

Each layer is a `Config`; `ConfigMerger::merge_multiple` folds them so later layers win per-field.

### Phase boundaries (where to gate features that aren't yet implemented)

- **SSH tunnel**: implemented in Phase 2 via `tunnel::SshTunnel` (russh-based). Single- and multi-hop jumps via comma-separated `--ssh-jump`; password or key auth; host keys accepted with stderr fingerprint warning. Strict known_hosts verification, key passphrases, and per-hop auth are Phase 3.
- **MCP server mode**: implemented in Phase 3. `main.rs` runs `mcp::serve_stdio` when no subcommand is given. Single tool `mysql_exec` (in `mcp::tools`) routes to `core::mysql::execute` — same execution path as `tools-mcp mysql "..."`.
- **Redis subcommand**: implemented in Phase 5. `tools-mcp redis "..."` and the `redis_exec` MCP tool both route through `core::redis::execute`. The `db` field is on `Config`/`Profile` for the Redis database number.
- **HTTP subcommand**: implemented in Phase 6. `tools-mcp http <METHOD> <URL>` and the `http_exec` MCP tool both route through `core::http::execute`. Tunnel routing uses reqwest's `resolve(host, addr)` override so HTTPS through SSH tunnels preserves SNI / Host header / cert verification. Phase 6 deliberately doesn't support Profile/YAML for HTTP — only CLI flags + global tunnel.
- **SSH-direct subcommand**: implemented in Phase 7. `tools-mcp ssh "<COMMAND>"` and the `ssh_exec` MCP tool both route through `core::ssh::execute`. TARGET credentials are separate from JUMP credentials (when `--tunnel=ssh` is used) — that's by design (Model A). Reuses the Phase 2 multi-hop infrastructure via `tools_mcp_ssh::session::build_session_chain` (extracted from the bin in Phase 7 so both `SshTunnel` and `SshExec` share it). Phase 7 deliberately doesn't support Profile/YAML — only CLI flags + global tunnel.
- **Service trait + orchestrator crate split**: implemented in Phase 8. `Service` trait lives in `tools-mcp-core`; the four orchestrators (`MysqlOrchestrator`, `RedisOrchestrator`, `HttpOrchestrator`, `SshDirectOrchestrator`) live in the new `tools-mcp-orchestrator` crate, each implementing the trait. MySQL and Redis orchestrators have a typed `<Svc>Request::from_config(config, action) -> Result<Self>` constructor that consolidates host/user/etc. validation; HTTP and SSH-direct skip the `from_config` ctor because they don't take `Config` (Profile/YAML deferred). The bin's `src/{config,tunnel,core}/` directories are deleted; CLI and MCP layers build typed requests and dispatch via `<Svc>Orchestrator::execute(req, tunnel)`.
- **PostgreSQL + MongoDB support**: implemented in Phase 9. Two new lib crates (`tools-mcp-pgsql` wrapping `tokio-postgres`, `tools-mcp-mongo` wrapping the official `mongodb` 3.x driver) plus matching `PgsqlOrchestrator` / `MongoOrchestrator` (both impl `Service` with their own typed Request + `from_config` constructor). CLI gains `tools-mcp pgsql "..."` and `tools-mcp mongo "..."`; MCP gains `pgsql_exec` / `mongo_exec`. Profile/YAML 3-layer merge supported (pgsql + mongo are typed-database services like mysql/redis). Mongo command syntax: JSON document passed to `Database::run_command`; result Document serialized to JSON in a single `result` row. Pgsql type mapping: bool / int2-8 / float4-8 / text / varchar / bpchar / name / date / time / timestamp(tz); other types render as `<typename>` placeholders.

## Conventions worth knowing

- **Cargo edition is `2024`** (not `2021` as the Phase 1 plan text says). Don't "fix" it back.
- **YAML crate is `serde_yml`** (the maintained fork). `serde_yaml` was deprecated upstream and removed during Phase 1 cleanup.
- **`lib.rs` declares only modules whose files exist.** When adding a new module file, also add its `pub mod X;` line. The Phase 1 plan listed all 7 modules upfront — that pattern fails to compile.
- **`--tunnel` is a flag, not a subcommand.** The original plan had a second `#[command(subcommand)]` alongside `command`, which clap rejects. Correct shape: `--tunnel direct|ssh` (`ValueEnum`) + flat `--ssh-*` global flags inside `SshTunnelArgs`. Match this when adding more tunnel kinds.
- **Help layout**: `Usage: tools-mcp [GLOBAL OPTIONS] <cmd> [OPTIONS] ...` uses `[GLOBAL OPTIONS]` as a handwritten placeholder; the constant `USAGE_LEGEND` in `cli/args.rs` (rendered via `after_help` on root and each subcommand) maps placeholders to actual help-section names. Adding a global flag is just `global = true` + `help_heading`; the `override_usage` strings don't need updating.
- **Path arguments use `PathBuf`**, not `String` (e.g. `--config`).
- **`Error::source()`**: lives in `tools-mcp-core`. The wrapping variant for non-IO errors is `Error::Service(String)` — service-specific error types (`mysql_async::Error`, `russh::Error`, `serde_yml::Error`, `toml::de::Error`) are flattened to a string at the boundary so core stays dep-free. If a future service needs to expose typed inner errors through `source()`, prefer adding a typed variant to `tools-mcp-<service>::Error` and only flattening at the bin boundary.
- **`main.rs` prints errors via `Display` to stderr** (not `Debug`). If touching `main()`, keep the explicit `if let Err(e) = ... { eprintln!("Error: {e}"); exit(1); }` pattern.
- **Stray SSH flags with `--tunnel=direct`** are a runtime `Error::Config` (not silently ignored); see `cli_to_tunnel_config`.
- **CLI <-> MCP parity**: every CLI subcommand has (or will have) a paired MCP tool, and both delegate to the same `core::<service>` function. When adding a new subcommand, write the core function first, then wire CLI and MCP on top — never embed business logic in either presentation layer.
- **Service-specific Profile/YAML support is opt-in**: MySQL, PostgreSQL, Redis, and MongoDB use the 3-layer merge (TOML profile → YAML → CLI args); HTTP and SSH-direct currently don't (Phase 6/7 simplification). When adding a new service, decide upfront whether profile support is in scope; if yes, add the relevant fields to `Profile` and `Config` and a `build_config_<svc>` sibling in `cli/handler.rs`. If no, follow the HTTP/SSH pattern — orchestrator takes a typed request struct + `Option<TunnelConfig>` directly, no `Config` plumbing.
- **Two-credential separation for ssh-direct**: when SSH is BOTH the tunnel transport AND the target service (i.e. `tools-mcp ssh ... --tunnel=ssh`), the JUMP credentials (`ssh_user`/`ssh_password`/`ssh_key_path`) and the TARGET credentials (`user`/`password`/`key_path`) are independent. Don't infer one from the other. The session chain authenticates with jump creds; the final SSH session (over the last jump's direct-tcpip channel) authenticates with target creds.
- **Per-service config fields**: `Config` is a flat bag of all possible fields across services (`database` for MySQL, `db` for Redis, `key_path` for SSH, etc.). Each orchestrator picks out only what it needs. When adding a new service that requires a new field, add it to `Profile` and `Config` (and the `ConfigMerger::merge` `or` chain) — but only if existing fields can't carry the meaning.
- **Service trait + typed Request pattern**: every service orchestrator in `tools-mcp-orchestrator` implements `tools_mcp_core::Service` with `type Request = <Svc>Request` (or the existing typed input from the service lib, e.g. `HttpRequestSpec` / `SshExecRequest`). The unified signature is `async fn execute(Self::Request, Option<TunnelConfig>) -> Result<ExecutionResult>`. CLI and MCP layers MUST go through this — they construct the typed Request (validating + draining `Config` for mysql/pgsql/redis/mongo, building directly from flags/JSON for http/ssh) then dispatch. Never bypass the Service impl with a free function.

## Implementation methodology

Phase 1 was built with `superpowers:subagent-driven-development` against the written plan: each task is one focused commit using TDD (failing test → impl → green), followed by spec-compliance + code-quality review. When extending Phase 1 or starting Phase 2:

- Write a plan in `docs/superpowers/plans/` before touching code
- Tests come before implementation (`cargo test <name>` should fail until the impl exists)
- One commit per task, with the `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailer used in existing commits
