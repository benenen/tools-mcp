# Tools MCP Phase 16: `tunnel-serve` long-running tunnel daemon

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or executing-plans).
>
> **Prerequisite:** Phase 14 Phase 3 (`StreamLocalTunnel`) + Phase 14 Phase 2 (`SocksTunnel`) + the original Phase 2 (`SshTunnel`) — all three tunnel impls already exist and stay unchanged structurally.

**Goal:** Add `tools4a tunnel-serve` CLI subcommand that builds a tunnel, **binds to a fixed local address** (the operator chooses the port), and stays alive until SIGINT/SIGTERM. Functionally equivalent to `ssh -L` / `ssh -D` but reuses tools4a's russh-based impls + multi-hop chain.

**Motivation:** The existing per-call tunnel model is correct for MCP (each tool call atomic + stateless) but wasteful for interactive CLI loops — every `tools4a docker ps --tunnel=ssh ...` pays a fresh SSH handshake (~500ms-2s). With `tunnel-serve`, the operator opens the tunnel once and subsequent tool calls (tools4a's own OR a raw `docker -H tcp://127.0.0.1:N` OR `mysql -h 127.0.0.1 -P N`) connect to the already-bound port with zero overhead.

**Out of scope (deliberate):**
- Not a service manager — no systemd unit files, no PID file management. Operator runs it in a terminal / tmux / nohup.
- No reload on config change. SIGHUP is fatal like SIGINT. Kill + restart.
- No automatic reconnect on SSH drop. The accept loop dies; operator restarts. Reconnect logic is its own design problem (back-off, half-open detection, in-flight conn handling) and deferred.
- No multiple tunnels in one process. Run multiple `tunnel-serve` processes for multiple targets.
- No MCP tool counterpart. Daemon lifecycle doesn't fit MCP's stateless tool model.

---

## Three tunnel shapes (matches existing impls)

| `--type` value | Underlying impl | Maps to ssh CLI | Listen behavior |
|---|---|---|---|
| `ssh-tcp` | `SshTunnel` | `ssh -L LPORT:HOST:PORT JUMP` | TCP, one fixed target |
| `ssh-streamlocal` | `StreamLocalTunnel` | `ssh -L LPORT:/sock JUMP` | TCP, one fixed remote unix socket |
| `ssh-socks` | `SocksTunnel` | `ssh -D LPORT JUMP` | SOCKS5 dynamic, many targets |

All three already exist and work — Phase 16 only adds (a) listen-addr override and (b) a CLI wrapper.

---

## Architecture

The current three tunnel impls all bind `127.0.0.1:0` (random port) unconditionally inside their `establish()`. To support a fixed listen address:

**Change 1 (minimal):** Each impl gets a new private field `listen_addr: Option<SocketAddr>` and a builder method:

```rust
impl SshTunnel {
    pub fn with_listen_addr(mut self, addr: SocketAddr) -> Self {
        self.listen_addr = Some(addr);
        self
    }
}
```

Inside `establish()`, replace `TcpListener::bind("127.0.0.1:0")` with:

```rust
let bind = self.listen_addr.unwrap_or_else(|| "127.0.0.1:0".parse().unwrap());
let listener = TcpListener::bind(bind).await?;
```

Same change in all three tunnel impls (`ssh.rs`, `streamlocal.rs`, `socks_tunnel.rs`). Existing callers don't set `listen_addr` and get the original random-port behavior — zero behavior change for Phase 11-15 code.

**Change 2:** New CLI variant `Commands::TunnelServe { ... }` in `cli/args.rs` and a matching `execute_tunnel_serve` in `cli/handler.rs`. The handler:

1. Parses `--type` + `--listen` + tunnel-specific args (target-host/port OR remote-socket).
2. Builds the right impl via `<X>::new(...).with_listen_addr(parsed)`.
3. Calls `establish()` — prints the bound `host:port` so the operator knows what to connect to.
4. Awaits `tokio::signal::ctrl_c()` (and `tokio::signal::unix::signal(SIGTERM)`).
5. On signal: prints "shutting down", calls `close()`, exits cleanly.

**No new MCP tool, no new leaf crate.** Bin-only change + a 3-line edit in each of the 3 tunnel files.

---

## CLI shape

```
# Equivalent to: ssh -L 2375:HOST:PORT JUMP
tools4a tunnel-serve --type ssh-tcp \
  --listen 127.0.0.1:13306 \
  --ssh-jump=10.6.125.14 --ssh-user=admin --ssh-password=admin \
  --target-host=mysql.internal --target-port=3306

# Equivalent to: ssh -L 2375:/var/run/docker.sock JUMP
tools4a tunnel-serve --type ssh-streamlocal \
  --listen 127.0.0.1:2375 \
  --ssh-jump=172.31.169.108 --ssh-port=2222 \
  --ssh-user=root --ssh-password=Iflysse@123 \
  --remote-socket=/var/run/docker.sock

# Equivalent to: ssh -D 1080 JUMP
tools4a tunnel-serve --type ssh-socks \
  --listen 127.0.0.1:1080 \
  --ssh-jump=10.6.125.14 --ssh-user=admin --ssh-password=admin
```

Flags:
- `--type {ssh-tcp,ssh-streamlocal,ssh-socks}` — required
- `--listen HOST:PORT` — required (e.g. `127.0.0.1:2375`)
- `--ssh-jump`, `--ssh-user`, `--ssh-password`/`--ssh-key-path`, `--ssh-port` — SSH chain (multi-hop via comma-separated `--ssh-jump=h1,h2`)
- `--target-host`, `--target-port` — for `ssh-tcp` only; rejected otherwise with `Error::Config`
- `--remote-socket` — for `ssh-streamlocal` only; rejected otherwise

These are **subcommand-local**, not reusing the global `--tunnel`/`--ssh-*` flags. Rationale: the global `--ssh-*` flags exist because every other subcommand does "X over an optional tunnel". `tunnel-serve` IS the tunnel; tying it to the global flags would force the operator to think "wait, is this a tunnel to call docker through, or is this a tunnel-serve?" — clearer to make them local.

---

## File Structure

**Modified (3 lines each):**
- `crates/tools4a-core/src/tunnel/ssh.rs`
- `crates/tools4a-core/src/tunnel/streamlocal.rs`
- `crates/tools4a-core/src/tunnel/socks_tunnel.rs`

Each: add `listen_addr: Option<SocketAddr>` field default `None`, set inside `new()` to `None`, add `with_listen_addr(self, SocketAddr) -> Self` method, replace the `bind("127.0.0.1:0")` line.

**Modified (bin):**
- `src/cli/args.rs` — add `TunnelServeType` enum + `TunnelServe { ... }` variant
- `src/cli/handler.rs` — `execute_tunnel_serve` with signal-driven shutdown
- `src/cli/mod.rs` — re-export `TunnelServeType`

**Docs:**
- `README.md`, `CLAUDE.md`, `AGENTS.md` — mark Phase 16 done

**No new files, no new crates.**

---

## Tasks

1. Modify the 3 tunnel impls — add `listen_addr` field + `with_listen_addr` method + consume in `establish()`. Existing tests stay green (none touch listen_addr; they get default `None` → 127.0.0.1:0).
2. Add `Commands::TunnelServe` + `TunnelServeType` enum to `cli/args.rs`.
3. Implement `execute_tunnel_serve` in `cli/handler.rs` with `tokio::signal::ctrl_c` shutdown.
4. `make ci` green.
5. Manual smoke against `172.31.169.108:2222`:
   - Open `--type ssh-streamlocal --listen 127.0.0.1:2375 --remote-socket=/var/run/docker.sock` in one terminal.
   - In another terminal: `docker -H tcp://127.0.0.1:2375 ps` returns the `dev` container.
   - Ctrl-C cleanly shuts down.

---

## Commit style

Single commit:

```
feat(cli/tunnel): add tunnel-serve subcommand for long-running tunnels (Phase 16)

New `tools4a tunnel-serve --type {ssh-tcp,ssh-streamlocal,ssh-socks}
--listen H:P ...` builds the right tunnel impl, binds to a fixed
local address, and stays alive until SIGINT. Equivalent to `ssh -L`
/ `ssh -D` but reuses tools4a's russh-based chain + multi-hop support.

Each tunnel impl gains an opt-in `with_listen_addr(SocketAddr)` builder;
when unset, the existing 127.0.0.1:0 random-port behavior holds. So
all current callers (orchestrators in MCP / CLI per-call mode) are
unchanged.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```
