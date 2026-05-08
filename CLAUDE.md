# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> 本文件与同目录的 [`AGENTS.md`](./AGENTS.md) 内容保持一致。修改任意一份时，请同步另一份。`AGENTS.md` 是给非 Claude 的 AI 编辑器（Cursor / Copilot / Codex 等）使用的等价文档。

## Project Overview

`tools-mcp` is a Rust CLI + MCP server for SSH, MySQL, and Redis. **Phase 3 (current) implements MySQL CLI mode + MCP server mode with the `mysql_exec` tool**; Redis and SSH direct are explicit phase boundaries (see below). The design spec lives at `docs/superpowers/specs/2026-05-07-tools-mcp-design.md` and the Phase 1 plan at `docs/superpowers/plans/2026-05-07-tools-mcp-phase1.md`.

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

**Workspace.** The repo is a Cargo workspace. The `tools-mcp` binary crate lives at the repo root (`./Cargo.toml` is both the workspace manifest and the bin's `[package]`); the two lib crates `tools-mcp-core` (service-agnostic traits + shared types) and `tools-mcp-mysql` (MySQL-specific, owns `mysql_async`) live under `crates/`. The bin wires them up plus the CLI/MCP/config presentation. Adding a new service (Redis, SSH-direct) means a new sibling lib crate (`crates/tools-mcp-redis`, …) plus a tunnel-dependent orchestrator in the bin.

### Module map (each module = one responsibility)

| Crate / Module | Role |
| --- | --- |
| `tools-mcp-core` (lib) | `Tunnel` / `Connection` async traits, `TunnelEndpoint`, `Error`/`Result` (with `Service(String)` for wrapped library errors), `ExecutionResult`. Sole external deps: `async-trait` + `serde`. The dependency floor for the workspace. |
| `tools-mcp-mysql` (lib) | `MySQLConnection` (impl `core::Connection`), `MySQLExecutor`, and the entry `execute(tunnel, params, query) -> ExecutionResult`. Owns the `mysql_async` dep. Service-agnostic about how the tunnel was built. |
| `tools-mcp` bin (root `src/cli/*`) | clap `Cli`, `SshTunnelArgs`, `CliHandler` — CLI mode parse + dispatch. |
| `tools-mcp` bin (root `src/mcp/*`) | rmcp `ServerHandler`, `mysql_exec` tool wiring, params → `Config` conversion. |
| `tools-mcp` bin (root `src/config/*`) | `Config`, `Profile`, `TunnelConfig`, `ConfigLoader`, `ConfigMerger`. Three-layer merge logic. |
| `tools-mcp` bin (root `src/tunnel/{direct,ssh}.rs`) | `DirectTunnel` and `SshTunnel` (russh) — the actual `Tunnel` trait impls. Stay in the bin so `tools-mcp-core` stays russh-free. |
| `tools-mcp` bin (root `src/core/mysql.rs`) | Orchestrator `execute(Config, &str)`: validate Config, build the right tunnel, translate to `tools_mcp_mysql::MysqlParams`, call into the lib. CLI handler and MCP tool both delegate here. |
| `tools-mcp` bin (root `src/output/cli.rs`) | `CliFormatter` — comfy-table renderer for CLI mode. Operates on `tools_mcp_core::ExecutionResult`. |

### Config priority (low → high)

1. `[profiles.<NAME>]` from `~/.config/tools-mcp/config.toml` when `--profile <NAME>` is set
2. YAML file when `--config <PATH>` is set
3. CLI args

Each layer is a `Config`; `ConfigMerger::merge_multiple` folds them so later layers win per-field.

### Phase boundaries (where to gate features that aren't yet implemented)

- **SSH tunnel**: implemented in Phase 2 via `tunnel::SshTunnel` (russh-based). Single- and multi-hop jumps via comma-separated `--ssh-jump`; password or key auth; host keys accepted with stderr fingerprint warning. Strict known_hosts verification, key passphrases, and per-hop auth are Phase 3.
- **MCP server mode**: implemented in Phase 3. `main.rs` runs `mcp::serve_stdio` when no subcommand is given. Single tool `mysql_exec` (in `mcp::tools`) routes to `core::mysql::execute` — same execution path as `tools-mcp mysql "..."`.
- **Redis / SSH-direct subcommands**: not yet implemented. When added, mirror the existing pattern: a `core::<service>` execution function, a CLI subcommand under `cli::Commands`, and an MCP tool in `mcp::tools` that delegates to the core. CLI and MCP must share the core; never duplicate execution logic in MCP land.

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

## Implementation methodology

Phase 1 was built with `superpowers:subagent-driven-development` against the written plan: each task is one focused commit using TDD (failing test → impl → green), followed by spec-compliance + code-quality review. When extending Phase 1 or starting Phase 2:

- Write a plan in `docs/superpowers/plans/` before touching code
- Tests come before implementation (`cargo test <name>` should fail until the impl exists)
- One commit per task, with the `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailer used in existing commits
