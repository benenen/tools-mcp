# Tools MCP Phase 14: Browser Support (Phase 3 — `StreamLocalTunnel` primitive)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or executing-plans). Single task — one focused commit.
>
> **Prerequisite:** Phase 14 Phase 2 (`SocksTunnel`) merged.

**Goal:** Add a new `Tunnel` impl — `StreamLocalTunnel` — that listens on a localhost TCP port and forwards each accepted TCP connection through an SSH `direct-streamlocal@openssh.com` channel to a remote Unix domain socket path. This is the "TCP locally, Unix socket remotely" shape, equivalent to OpenSSH's `ssh -L 127.0.0.1:N:/var/run/foo.sock host`. Use case driver: reaching `/var/run/docker.sock` (and other Unix-socket-only services like `mysqld.sock`, agent sockets, etc.) through an SSH chain, callable from any tools4a service that consumes a TCP `TunnelEndpoint` (i.e. all current services).

**Scope (this phase, deliberately minimal):**
- New tunnel impl in `tools4a-core::tunnel::streamlocal`, exported alongside `SshTunnel` / `SocksTunnel`.
- `Tunnel` trait surface unchanged. `TunnelEndpoint` unchanged (local side is still TCP).
- `TunnelConfig` enum unchanged — no new variant. Users select this tunnel by orchestrators that opt in, **not** via a TunnelConfig field. (Per the design note in CLAUDE.md: "Do NOT add a new `TunnelConfig` variant to expose the shape distinction". Mirrors how `BrowserOrchestrator` chooses `SocksTunnel` over `SshTunnel` internally.)
- `build_tunnel()` helper **not modified** — it stays as `(host, port, tunnel_config)`. Wiring into specific orchestrators is a separate future phase.
- No service is wired up in this phase. `http_exec` etc. don't get a `unix_socket` field yet. That's deferred to a follow-up so this PR is reviewable.

**Architecture:** Same shape as `SshTunnel` / `SocksTunnel` lifecycle:

1. `new(ssh_jumps, ssh_user, ssh_password, ssh_key_path, ssh_port, remote_socket_path)` — record config, no IO.
2. `establish()` — build SSH session chain via shared `build_session_chain`; bind `127.0.0.1:0`; spawn accept loop where each accept opens a fresh `Handle::channel_open_direct_streamlocal(socket_path)` and bridges with `tokio::io::copy_bidirectional`. Returns `TunnelEndpoint { host: "127.0.0.1", port: <random> }`.
3. `close()` — watch-channel shutdown signal + JoinHandle await. Sessions drop with the state.

This is structurally `SshTunnel` minus the `(target_host, target_port)` channel args, replaced by a single `socket_path` arg. Most of the code is a near-copy of `tunnel/ssh.rs` with two changes:
- Constructor takes `remote_socket_path: String` instead of `(target_host, target_port)`.
- `forward_one` calls `channel_open_direct_streamlocal(&socket_path)` instead of `channel_open_direct_tcpip(host, port, "127.0.0.1", 0)`.

**Why this design (vs. alternatives):**
- *Not* a `TunnelConfig::SshSocket` variant. Per CLAUDE.md: tunnel **shape** is an orchestrator-internal detail, not a user-facing knob. A user picks "tunnel = ssh" with credentials; the orchestrator picks the impl based on what its protocol needs.
- *Not* a refactor of `SshTunnel` to take a `Target = Tcp(host,port) | Unix(path)` enum. That's premature — two impls is fine; the duplication is ~30 lines of accept-loop boilerplate that's already replicated between `SshTunnel` and `SocksTunnel`. Three is fine too.
- *Not* `TunnelEndpoint::Unix(PathBuf)`. That would touch all 7 leaf crates and gain nothing — every consumer would still pick TCP because that's what its protocol speaks. Local-side TCP keeps `http_exec` etc. zero-change-cost when they eventually opt in.
- *Not* exposing `streamlocal-forward@openssh.com` (the remote→local reverse direction). That's a separate impl and there's no current use case.

**Out of scope (deferred):**
- Wiring into any service orchestrator. Follow-up phase will add `unix_socket: Option<String>` to `HttpExecParams` (and possibly other tools where it's meaningful) and build a `StreamLocalTunnel` instead of `SshTunnel` when set. Keeping that out of this phase to avoid mixing primitive work with service-API design.
- Local-side Unix socket bind (`/local.sock` instead of `127.0.0.1:N`). Would require extending `TunnelEndpoint` and is unnecessary for tools4a's current consumer surface.
- E2E test against a real sshd. Unit tests cover construction + state-machine; full happy-path requires a real SSH server (manual smoke per Phase 2 Task 8 precedent).

---

## File Structure

**New:**
- `crates/tools4a-core/src/tunnel/streamlocal.rs` — `StreamLocalTunnel: impl Tunnel`.

**Modified:**
- `crates/tools4a-core/src/tunnel/mod.rs` — `mod streamlocal; pub use streamlocal::StreamLocalTunnel;`.
- `crates/tools4a-core/src/lib.rs` — re-export `StreamLocalTunnel` from `tunnel`.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — mark Phase 14 Phase 3 done (one paragraph each).

---

## Task 1: `StreamLocalTunnel` impl + unit tests

**Files:**
- Create: `crates/tools4a-core/src/tunnel/streamlocal.rs`
- Modify: `crates/tools4a-core/src/tunnel/mod.rs`
- Modify: `crates/tools4a-core/src/lib.rs`

Closely mirror `tools4a-core/src/tunnel/ssh.rs` — same lifecycle, same shutdown pattern, same `Arc<Mutex<Handle>>` retention, same `Box::pin(channel.into_stream())` for `copy_bidirectional`. Differences:

1. Constructor signature: `new(ssh_jumps, ssh_user, ssh_password, ssh_key_path, ssh_port, remote_socket_path: String)`.
2. `establish()`: replace `target_host` / `target_port` plumbing with a single `remote_socket_path: String` that the per-accept task captures.
3. `forward_one`: call `session.lock().await.channel_open_direct_streamlocal(socket_path).await` instead of `channel_open_direct_tcpip(...)`.
4. Error message in `forward_one` should mention the socket path so failures are debuggable.

**Validation:** empty `ssh_jumps` → `Error::Config`. Empty `remote_socket_path` → `Error::Config` (different from SshTunnel which validates host non-emptiness at the SSH-chain layer; remote socket path empty is meaningless and worth a fast fail).

**Tests** (mirrors `tunnel/ssh.rs` and `tunnel/socks_tunnel.rs`):
- `test_new_rejects_empty_jumps` — empty `ssh_jumps` returns `Error::Config`.
- `test_new_rejects_empty_socket_path` — empty `remote_socket_path` returns `Error::Config`.
- `test_new_accepts_valid_input` — construction succeeds, `is_active()` is false.
- `test_state_starts_inactive` — async test confirming `is_active()` is false before `establish()`.

No `establish()` integration test — same rationale as `SshTunnel` / `SocksTunnel`: needs a real sshd.

**`tunnel/mod.rs` change:**

```rust
mod direct;
mod socks_tunnel;
mod ssh;
mod streamlocal;            // ← new
pub mod socks;

pub use direct::DirectTunnel;
pub use socks_tunnel::SocksTunnel;
pub use ssh::SshTunnel;
pub use streamlocal::StreamLocalTunnel;   // ← new
```

`build_tunnel()` is **not** modified.

**`lib.rs` change:**

```rust
pub use tunnel::{DirectTunnel, SocksTunnel, SshTunnel, StreamLocalTunnel, build_tunnel};
```

---

## Manual smoke (operator-side, post-merge)

Not part of CI. The Phase-2 plan's Task 8 documents this style; same idea here:

```rust
// In a throwaway main.rs or example/:
use tools4a_core::{StreamLocalTunnel, Tunnel};

let mut t = StreamLocalTunnel::new(
    vec!["jump.example.com".into()],
    "admin".into(),
    Some("password".into()),
    None,
    22,
    "/var/run/docker.sock".into(),
)?;
let ep = t.establish().await?;
println!("local: 127.0.0.1:{}", ep.port);

// Query Docker via the tunnel:
let res = reqwest::get(format!("http://127.0.0.1:{}/containers/json", ep.port)).await?;
println!("{}", res.text().await?);

t.close().await?;
```

Expected: `containers/json` returns the remote Docker daemon's container list.

---

## Commit

One commit, message style matches Phase 14 Phase 2's final commit:

```
feat(tunnel): add StreamLocalTunnel for SSH→Unix-socket forwarding (Phase 14 Phase 3)

New `tools4a-core::tunnel::StreamLocalTunnel` binds a local TCP listener on
127.0.0.1:0 and forwards each accepted connection through an SSH chain via
`direct-streamlocal@openssh.com` to a remote Unix socket path. Equivalent
to `ssh -L 127.0.0.1:N:/var/run/foo.sock host`. Trait surface unchanged;
TunnelConfig enum unchanged; no orchestrator wired up in this phase.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```
