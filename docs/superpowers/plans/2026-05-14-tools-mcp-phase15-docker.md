# Tools MCP Phase 15: Docker leaf crate

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or executing-plans).
>
> **Prerequisite:** Phase 14 Phase 3 (`StreamLocalTunnel`) merged. This phase is the first consumer.

**Goal:** Add a `tools4a-docker` leaf crate that talks to the Docker Engine API, with three connection modes:
1. **Local Unix socket** (`unix:///var/run/docker.sock`) — direct, no tunnel.
2. **Local or remote TCP** (`tcp://host:port`) — direct or via `SshTunnel`.
3. **Remote Unix socket via SSH** — via the just-landed `StreamLocalTunnel`.

Typed surface: bollard's typed API (per the user decision; bollard's internal request entry is `pub(crate)` so a generic raw method+path tool isn't possible without forking). Each common operation becomes its own MCP tool to give LLMs clean parameter shapes.

**MCP tools to ship in Phase 15 (7 tools):**

| Tool | Purpose | Bollard call | Read/Write |
|---|---|---|---|
| `docker_ps` | List containers (with filters) | `Docker::list_containers` | read |
| `docker_inspect` | Container details | `Docker::inspect_container` | read |
| `docker_logs` | Tail / fetch container logs | `Docker::logs` | read |
| `docker_stats` | One-shot resource stats snapshot | `Docker::stats` (`stream=false`) | read |
| `docker_top` | Process list inside container | `Docker::top_processes` | read |
| `docker_exec` | Run a command inside a container | `create_exec` + `start_exec` | write (`allow_write=true`) |
| `docker_restart` | Restart a container | `Docker::restart_container` | write |

Read-only by default — write tools require `allow_write=true` (Phase 10 pattern).

**CLI:** `tools4a docker <sub> [OPTS]` with nested clap subcommands matching the 7 MCP tools (`ps`, `inspect`, `logs`, `stats`, `top`, `exec`, `restart`).

---

## Architecture

Standard leaf-crate shape (per Phase 11):

- `crates/tools4a-docker/src/connection.rs` — builder that wraps `bollard::Docker`; picks between `connect_with_unix`, `connect_with_http`, and the tunneled "127.0.0.1:N" form.
- `crates/tools4a-docker/src/actions.rs` — Per-action functions: `do_ps`, `do_inspect`, `do_logs`, `do_stats`, `do_top`, `do_exec`, `do_restart`. Each takes `&Docker` + typed args, returns rows for `ExecutionResult`.
- `crates/tools4a-docker/src/run.rs` — `run(tunnel, request, allow_write)` — single dispatcher that switches on the `DockerAction` enum and calls the right action function. (Named `run.rs` rather than `execute.rs` because the latter clashes with the `Service` trait method name in this leaf.)
- `crates/tools4a-docker/src/orchestrator.rs` — `DockerRequest` + `DockerOrchestrator: impl Service`. The `DockerRequest` carries `action: DockerAction`, `docker_host: String`, `unix_socket: Option<String>` (for `StreamLocalTunnel` selection), `allow_write: bool`, `timeout_secs`.
- `crates/tools4a-docker/src/mcp.rs` — Seven MCP tool types (`DockerPsMcp`, `DockerInspectMcp`, …), each impl `McpTool` with its own typed `Params`. Each builds a `DockerRequest` with the right `DockerAction` variant and dispatches through `DockerOrchestrator`.

**Why 7 McpTool impls instead of 1**: bollard's typed API means each operation has different params (e.g. `tail` for logs, `cmd` for exec). Folding into a single tool with optional fields per action would be a worse LLM UX than seven small tools. We share machinery via the orchestrator + run.rs layer.

---

## Connection-mode selection (the interesting bit)

The orchestrator resolves the connection target like this (pseudocode):

```text
match (tunnel_config, unix_socket):
  (Some(Ssh{..}), Some(socket_path)):
      build StreamLocalTunnel(ssh chain, socket_path)
      establish -> 127.0.0.1:N
      bollard target = "127.0.0.1:N"
  (Some(Ssh{..}), None):
      parse docker_host as "tcp://host:port"
      build SshTunnel(ssh chain, host, port)
      establish -> 127.0.0.1:N
      bollard target = "127.0.0.1:N"
  (None|Direct, None):
      if docker_host starts with "unix://":
          bollard target = UnixSocket(path)
      else:
          bollard target = Tcp(docker_host)
  (None|Direct, Some(_)):
      reject -> Error::Config "unix_socket only valid with tunnel=ssh"

if bollard target is UnixSocket:  Docker::connect_with_unix(path, ...)
else:                              Docker::connect_with_http(addr, ...)

dispatch on action; tear tunnel down on return.
```

Conflict guard: `unix_socket=Some(_)` requires `tunnel=ssh`. Otherwise → `Error::Config`.

---

## Read-only gating

```
is_readonly(action) := action in {Ps, Inspect, Logs, Stats, Top}
```

`DockerOrchestrator` rejects `!is_readonly && !allow_write` with `Error::Config`. Matches MySQL/Postgres/Mongo pattern (Phase 10).

---

## File Structure

**New crate:**
- `crates/tools4a-docker/Cargo.toml` — deps: `bollard 0.21` (default features, gives `http` + `pipe` = TCP + unix socket), `tools4a-core`, `tokio`, `async-trait`, `serde`, `serde_json`, `schemars`, `futures-util` (for `Stream` collection on logs/stats).
- `crates/tools4a-docker/src/lib.rs` — module declarations + re-exports.
- `crates/tools4a-docker/src/connection.rs`
- `crates/tools4a-docker/src/actions.rs`
- `crates/tools4a-docker/src/run.rs`
- `crates/tools4a-docker/src/orchestrator.rs`
- `crates/tools4a-docker/src/mcp.rs`

**Modified:**
- `Cargo.toml` (workspace) — register the new crate.
- `Cargo.toml` (bin `tools4a`) — add `tools4a-docker` dep.
- `src/cli/args.rs` — add `DockerCommand` enum with seven subcommands.
- `src/cli/handler.rs` — dispatch `DockerCommand` (build `DockerRequest` from CLI args).
- `src/mcp/server.rs` — seven `#[tool]` methods, each one-liner: `into_call_result(<X>Mcp::invoke(params).await)`.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — document Phase 15.

**Not modified:**
- `tools4a-core` — no changes. `StreamLocalTunnel` is already exported from Phase 14 Phase 3.
- The other 7 leaf crates — untouched.

---

## Task breakdown (single commit OK if work fits)

1. Create `tools4a-docker` Cargo.toml + lib.rs skeleton; add to workspace; `cargo build` passes (empty modules).
2. Implement `connection.rs` (returns a connected `bollard::Docker` given a target enum); unit test connection-target parsing (`unix://...` vs `tcp://...`).
3. Implement `actions.rs` (7 functions) + `run.rs` (dispatch); shape `ExecutionResult` rows per action.
4. Implement `orchestrator.rs` with `Service` impl, including `StreamLocalTunnel` selection logic and `allow_write` gating. Unit tests: read-only-without-allow-write rejection, `unix_socket` + non-ssh-tunnel rejection.
5. Implement `mcp.rs`: seven `McpTool` impls + their `Params` types.
6. Wire into bin: `cli/args.rs` (clap subcommands), `cli/handler.rs` (dispatch), `mcp/server.rs` (seven `#[tool]` methods).
7. Docs: short README mention + Phase 15 line in CLAUDE.md / AGENTS.md.
8. `make ci` passes.

Estimated implementation: ~600-900 lines new + ~80 lines bin wiring.

---

## Out of scope (deferred)

- **TLS Docker daemons** (port 2376). Enable bollard's `ssl` feature later when someone has a real cert pair.
- **Long-lived streams**: `docker_stats --stream`, `docker_events`, `docker_logs --follow=true`. The Phase 15 implementations all use one-shot mode (`stream=false`, follow=false). Streaming inside MCP needs a different result shape (we'd return a paginated tail) — defer until there's a concrete use case.
- **Image and network operations**: `docker_images`, `docker_image_pull`, `docker_network_*`. Add as separate MCP tools later when needed.
- **Profile/YAML 3-layer merge for docker host**: Phase 15 keeps it simple (CLI args + MCP params only), like the `http` and `ssh` leaves. The 3-layer config is only for typed-DB services with stable connection params.
- **HTTP leaf getting `unix_socket` field**: The other promised consumer of `StreamLocalTunnel`. Pure HTTP-over-StreamLocalTunnel still works today (operator can rig a manual call), but the tool-level `unix_socket` field is a separate small phase.

---

## Commit style

One commit if it fits cleanly. Conventional commit scope `feat(docker):` matches the project pattern. If it splits naturally (skeleton → connection → actions → orchestrator/mcp → bin wiring), prefer 2-3 commits, one per task batch.
