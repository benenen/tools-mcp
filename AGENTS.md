# AGENTS.md

This file provides guidance to AI coding agents (Cursor, Copilot, Codex, etc.) when working with code in this repository.

> 本文件与同目录的 [`CLAUDE.md`](./CLAUDE.md) 内容保持一致。修改任意一份时，请同步另一份。`CLAUDE.md` 是给 Claude Code 使用的等价文档。

## Project Overview

`tools-mcp` is a Rust CLI / future MCP server for SSH, MySQL, and Redis. **Phase 1 (current) implements only MySQL CLI mode** with config support; SSH tunnel, Redis, SSH direct, and MCP server mode are explicit phase boundaries (see below). The design spec lives at `docs/superpowers/specs/2026-05-07-tools-mcp-design.md` and the Phase 1 plan at `docs/superpowers/plans/2026-05-07-tools-mcp-phase1.md`.

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

### Module map (each module = one responsibility)

| Module | Role |
| --- | --- |
| `error` | `Error` enum with `source()` chain preservation; wrapping variants (`Io`/`Mysql`/`Yaml`/`Toml`) return their inner cause |
| `config::types` | `Config` (merged runtime), `Profile` (TOML entry, `service_type` non-optional), `TomlConfig`, `TunnelConfig` (tagged enum: `Direct` unit / `Ssh` struct), `ServiceType` |
| `config::loader` | `ConfigLoader::load_{toml,yaml}_file(&Path)` and `load_default_toml()`; honors `XDG_CONFIG_HOME`, falls back to `$HOME/.config/tools-mcp/config.toml`. Errors include the file path. |
| `config::merger` | `ConfigMerger::merge_multiple(Vec<Config>)` folds with `Option::or` per field — later configs override earlier |
| `cli::args` | clap `Cli`; SSH-specific flags live in `SshTunnelArgs` flattened via `#[command(flatten)]`; `--tunnel direct\|ssh` is a `ValueEnum` |
| `cli::handler` | three-layer config merge in `build_config`; `cli_to_tunnel_config` validates SSH constraints |
| `tunnel::{traits,direct}` | async `Tunnel` trait (`establish/close/is_active`); `DirectTunnel` only — SSH is Phase 2 |
| `connection::{traits,mysql}` | async `Connection` trait; `MySQLConnection` takes `Box<dyn Tunnel>` and opens a `mysql_async::Pool` from `tunnel.establish()`'s endpoint |
| `executor::mysql` | `MySQLExecutor::execute(&mut MySQLConnection, &str)` — query + Value→String |
| `output::{types,cli}` | `ExecutionResult { columns, rows, affected_rows }`; `CliFormatter` renders a `comfy-table` UTF-8 box |

### Config priority (low → high)

1. `[profiles.<NAME>]` from `~/.config/tools-mcp/config.toml` when `--profile <NAME>` is set
2. YAML file when `--config <PATH>` is set
3. CLI args

Each layer is a `Config`; `ConfigMerger::merge_multiple` folds them so later layers win per-field.

### Phase boundaries (where to gate features that aren't Phase 1)

- **SSH tunnel**: parses fine into `TunnelConfig::Ssh` but is rejected at runtime in `cli/handler.rs::execute_mysql` with `Error::Config("SSH tunnel is not yet implemented in Phase 1")`. When Phase 2 lands, replace that guard with the real `SshTunnel` construction.
- **MCP server mode**: triggered when no subcommand is given; `main.rs` prints a placeholder and exits 1.

## Conventions worth knowing

- **Cargo edition is `2024`** (not `2021` as the Phase 1 plan text says). Don't "fix" it back.
- **YAML crate is `serde_yml`** (the maintained fork). `serde_yaml` was deprecated upstream and removed during Phase 1 cleanup.
- **`lib.rs` declares only modules whose files exist.** When adding a new module file, also add its `pub mod X;` line. The Phase 1 plan listed all 7 modules upfront — that pattern fails to compile.
- **`--tunnel` is a flag, not a subcommand.** The original plan had a second `#[command(subcommand)]` alongside `command`, which clap rejects. Correct shape: `--tunnel direct|ssh` (`ValueEnum`) + flat `--ssh-*` global flags inside `SshTunnelArgs`. Match this when adding more tunnel kinds.
- **Help layout**: `Usage: tools-mcp [GLOBAL OPTIONS] <cmd> [OPTIONS] ...` uses `[GLOBAL OPTIONS]` as a handwritten placeholder; the constant `USAGE_LEGEND` in `cli/args.rs` (rendered via `after_help` on root and each subcommand) maps placeholders to actual help-section names. Adding a global flag is just `global = true` + `help_heading`; the `override_usage` strings don't need updating.
- **Path arguments use `PathBuf`**, not `String` (e.g. `--config`).
- **`Error::source()`**: when adding an error variant that wraps an underlying error, extend the exhaustive `match` so the cause chain stays intact.
- **`main.rs` prints errors via `Display` to stderr** (not `Debug`). If touching `main()`, keep the explicit `if let Err(e) = ... { eprintln!("Error: {e}"); exit(1); }` pattern.
- **Stray SSH flags with `--tunnel=direct`** are a runtime `Error::Config` (not silently ignored); see `cli_to_tunnel_config`.

## Implementation methodology

Phase 1 was built with `superpowers:subagent-driven-development` against the written plan: each task is one focused commit using TDD (failing test → impl → green), followed by spec-compliance + code-quality review. When extending Phase 1 or starting Phase 2:

- Write a plan in `docs/superpowers/plans/` before touching code
- Tests come before implementation (`cargo test <name>` should fail until the impl exists)
- One commit per task, with a `Co-Authored-By:` trailer matching the existing convention in this repo's commit history
