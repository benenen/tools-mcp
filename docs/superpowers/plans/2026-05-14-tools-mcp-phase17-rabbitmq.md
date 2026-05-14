# Tools MCP Phase 17: RabbitMQ Management API leaf crate

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or executing-plans).
>
> **Prerequisite:** Phase 15 (docker leaf) merged. Same leaf-crate shape applies.

**Goal:** Add a `tools4a-rabbitmq` leaf crate that talks to the RabbitMQ **Management HTTP API** (default port 15672). Ships 5 MCP tools focused on diagnosis (list queues, peek messages, list bindings, single queue detail, cluster overview). Supports plain HTTP + HTTPS with optional `--insecure`, and routes through SSH tunnel when needed.

**Not in scope:** AMQP 0-9-1 protocol (publish/consume). RabbitMQ's wire protocol is a long-lived consumer model that doesn't fit MCP's stateless tool semantics. If someone needs AMQP, lapin in a separate crate is the path.

**MCP tools to ship in Phase 17 (5 tools, all read-only):**

| Tool | Purpose | REST endpoint |
|---|---|---|
| `rabbitmq_list_queues` | List queues + key counters (ready/unacked/consumers/rates). Pattern filter. | `GET /api/queues[/{vhost}]` |
| `rabbitmq_queue_info` | Single queue detail (full JSON). | `GET /api/queues/{vhost}/{name}` |
| `rabbitmq_get_messages` | Peek N messages without consuming (ackmode=ack_requeue_true). | `POST /api/queues/{vhost}/{name}/get` |
| `rabbitmq_list_bindings` | List bindings (optionally filter by source / destination). | `GET /api/bindings[/{vhost}]` |
| `rabbitmq_overview` | Cluster + node overview (msg rates, totals, listeners). | `GET /api/overview` |

All read-only — no `allow_write` gating in v1. `get_messages` always uses `ack_requeue_true` so peeking is non-destructive. Write actions (purge_queue, delete_*) deferred to Phase 17b.

---

## Architecture

Standard leaf-crate shape (per Phase 11):

- `connection.rs` — builds a `reqwest::Client` configured for tunneled or direct connections. Owns the base URL builder, basic-auth header, vhost URL encoding (`/` → `%2F`), and the resolve override that points the URL host at the tunnel's local TCP port while preserving Host / SNI / cert verification (same trick http leaf uses).
- `actions.rs` — 5 action functions. Each takes `&RabbitmqConnection` + typed args, calls one or two HTTP endpoints, parses JSON, shapes into `ExecutionResult` rows.
- `run.rs` — single `RabbitmqAction` enum dispatcher.
- `orchestrator.rs` — `RabbitmqRequest` + `RabbitmqOrchestrator: impl Service`. Resolves connection target (`http://host:port` vs `https://host:port`), builds tunnel via `build_tunnel` when needed, hands off to `run::run`.
- `mcp.rs` — 5 `McpTool` impls sharing a flat `RabbitmqConnectionFields` struct (host/port/scheme/user/password/vhost/insecure + tunnel fields).

Deps: `reqwest 0.12` (with `rustls-tls` + `gzip` features matching http leaf), `serde_json` for response parsing.

---

## Tunnel + TLS handling (the slightly tricky part)

Mirror http leaf's pattern:

1. Parse the user's `rabbitmq_host` + `rabbitmq_port` + `rabbitmq_scheme` (default `http`/15672).
2. If `tunnel=ssh`: build `SshTunnel(host, port)` → get local 127.0.0.1:N endpoint. Tell reqwest `resolve(real_host, 127.0.0.1:N)`. URL stays `https://real_host:port/api/...` so SNI + cert verify against the original host name still works.
3. If `insecure=true`: also set `danger_accept_invalid_certs(true)`. This unlocks two cases: (a) HTTPS to a host with a self-signed cert, (b) HTTPS through tunnel when the cert doesn't include 127.0.0.1.

Default port 15672 (HTTP) or 15671 (HTTPS) based on scheme.

---

## Vhost URL encoding

RabbitMQ's REST API URL-encodes vhost names. The default vhost `/` becomes `%2F`. Custom names like `myapp` stay as-is. Always run through `urlencoding::encode` (a one-function micro-crate, or inline via `percent_encoding` which reqwest already pulls in).

---

## Read-only / write split

Phase 17: all 5 tools are read-only by API semantics. `get_messages` POSTs but with `ack_requeue_true` it's a peek that puts the message back. No `allow_write` field on any tool.

Phase 17b (future, separate plan): `rabbitmq_purge_queue` (`DELETE /api/queues/{vhost}/{name}/contents`) with `allow_write=true` required. Maybe also `delete_queue` / `delete_exchange`.

---

## File Structure

**New crate:**
- `crates/tools4a-rabbitmq/Cargo.toml`
- `crates/tools4a-rabbitmq/src/lib.rs`
- `crates/tools4a-rabbitmq/src/connection.rs`
- `crates/tools4a-rabbitmq/src/actions.rs`
- `crates/tools4a-rabbitmq/src/run.rs`
- `crates/tools4a-rabbitmq/src/orchestrator.rs`
- `crates/tools4a-rabbitmq/src/mcp.rs`

**Modified (bin wiring, same shape as Phase 15 docker):**
- `Cargo.toml` (workspace) — add `crates/tools4a-rabbitmq` member + bin dep.
- `src/cli/args.rs` — add `Rabbitmq` variant with 5 subcommands.
- `src/cli/handler.rs` — `execute_rabbitmq` dispatcher.
- `src/cli/mod.rs` — re-export `RabbitmqCommand`.
- `src/mcp/server.rs` — 5 `#[tool]` methods.

**Docs:**
- `README.md` / `CLAUDE.md` / `AGENTS.md` — mark Phase 17.

---

## Tasks

1. Plan file.
2. Crate skeleton (Cargo.toml + lib.rs) + `connection.rs`.
3. Implement 5 actions + dispatcher in `actions.rs` / `run.rs`.
4. Implement `orchestrator.rs` (Service impl) + `mcp.rs` (5 McpTool impls).
5. Bin wiring (workspace + args + handler + server).
6. `make ci` green.

Estimated total: ~700 lines new + ~80 lines bin wiring.

---

## Commit style

One commit:

```
feat(rabbitmq): add tools4a-rabbitmq leaf crate (Phase 17)

New leaf crate wrapping the RabbitMQ Management HTTP API. Five MCP
tools: rabbitmq_list_queues, rabbitmq_queue_info, rabbitmq_get_messages
(non-destructive peek), rabbitmq_list_bindings, rabbitmq_overview.
All read-only in v1; write actions (purge, delete) deferred to a
follow-up phase. Supports HTTP + HTTPS with --insecure, and routes
through SshTunnel when tunnel=ssh is set. Reuses http leaf's
reqwest resolve() trick to preserve SNI/cert verify through the tunnel.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```
