# Tools MCP Phase 14: Browser Support (Phase 2 — SOCKS5 over SSH for the browser tool)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Prerequisite:** Phase 14 Phase 1 (browser leaf crate, `browser_exec` MCP tool, manual `ssh -D` + `--proxy` workaround) must be merged before starting this plan.

**Goal:** Make `tunnel=ssh` work for the browser tool the same way it works for mysql/pgsql/redis/mongo/http/ssh-exec. The path is: tools4a builds a SOCKS5 server backed by SSH `direct-tcpip` channels, binds to `127.0.0.1:<rand>`, injects `--proxy socks5://127.0.0.1:<rand>` into the agent-browser invocation, and tears the listener down on exit. From the user's perspective the experience matches the other six tools — pass `--tunnel=ssh --ssh-jump=bastion --ssh-user=admin`, the browser reaches the bastion-side network, done.

**Architecture:** New `tools4a-core::tunnel::socks` submodule that owns the SOCKS5 protocol code (greeting / request / reply codecs, the `Connector` trait, the `Socks5Server::serve_one` per-conn handler). New `SshConnector` adapter that opens a russh `direct-tcpip` channel for each SOCKS CONNECT request, bridging it to the inbound TCP socket with `tokio::io::copy_bidirectional`. New `SocksTunnel` that implements the existing `tools4a_core::Tunnel` trait: `establish()` binds a localhost TCP listener and spawns an accept loop; `close()` cleanly shuts the loop and the SSH session. The session chain itself is reused unchanged from Phase 7's `build_session_chain` — `SocksTunnel` and `SshTunnel` differ only in what happens after the chain is built (single forwarded port vs. on-demand channels per accepted SOCKS conn).

**Orchestrator wiring (browser-only):** `BrowserOrchestrator::execute` stops rejecting `TunnelConfig::Ssh`. Instead, when it receives one, it builds a `SocksTunnel`, calls `establish()`, gets a `TunnelEndpoint { host: "127.0.0.1", port: <rand> }`, and injects `proxy = format!("socks5://{host}:{port}")` into the `BrowserRequest` (erroring if the user already set a `proxy` — those conflict). After `BrowserExec::run` returns, the tunnel is closed in a `Drop` / explicit `close()` regardless of outcome. The other six service orchestrators are NOT touched — they continue to use `SshTunnel` (single-port `direct-tcpip`) because that's exactly the shape their wire protocols expect.

**Why this design (vs. alternatives):**
- *Not* a new `TunnelConfig::Socks` variant. Users shouldn't have to know that browser uses SOCKS while mysql uses single-port. The kind switch lives inside each orchestrator. `TunnelConfig::Ssh` is a credentials-and-chain spec; the orchestrator decides what to do with the chain.
- *Not* a global `--proxy` flag at the bin level. Keeps the CLI surface stable. The orchestrator injects the proxy via `BrowserRequest.proxy`, which is the same field a user could set manually — agent-browser only ever sees one mechanism.
- *Not* a fork or vendor of an external SOCKS5 crate (e.g. `fast-socks5`, `socks5-proto`). The protocol surface we need is small (~150 LOC of hand-rolled codec) and we already own a russh session — adding a dep with its own russh dependency hierarchy is more weight than the code we're avoiding.
- *Not* an opportunistic refactor of `SshTunnel` to share more code with `SocksTunnel`. They share `build_session_chain` (already extracted in Phase 7); below that, the lifecycle differs. Force-sharing more would create a leaky abstraction. Three lines of duplication beat a premature trait.

**Out of scope (deferred):**
- SOCKS4 / SOCKS4a (only SOCKS5; agent-browser/Chrome support SOCKS5 fine).
- SOCKS5 authentication methods other than "no auth" (the listener is bound to `127.0.0.1` only; password auth is meaningless on localhost).
- UDP ASSOCIATE / BIND (only CONNECT). Browsers only need CONNECT.
- IPv6 listener (bind on `127.0.0.1`; if a user needs `::1`, file an issue).
- DNS resolution inside the tunnel — agent-browser sets `socks5://` (Chrome's `--proxy-server=socks5://host:port` resolves DNS via the proxy by default), so we send a Domain ATYP through, the SOCKS server hands the domain to russh's direct-tcpip channel, and the bastion resolves it. No client-side resolution.
- SOCKS for the other six services. The single-port `SshTunnel` is correct for them; only browser benefits from SOCKS.
- Per-call SOCKS connection limit / rate limiting.

---

## File Structure

**New:**
- `crates/tools4a-core/src/tunnel/socks/mod.rs` — re-exports.
- `crates/tools4a-core/src/tunnel/socks/codec.rs` — SOCKS5 protocol encoding/decoding (pure functions over bytes).
- `crates/tools4a-core/src/tunnel/socks/connector.rs` — `Connector` async trait + `SshConnector` impl.
- `crates/tools4a-core/src/tunnel/socks/server.rs` — `Socks5Server::serve_one(conn, connector)` per-connection handler.
- `crates/tools4a-core/src/tunnel/socks_tunnel.rs` — `SocksTunnel: impl Tunnel` (bind listener + accept loop + close).

**Modified:**
- `crates/tools4a-core/src/tunnel/mod.rs` — `pub mod socks; pub mod socks_tunnel;` + re-export `SocksTunnel`.
- `crates/tools4a-core/Cargo.toml` — no new deps (russh + tokio + async-trait already present).
- `crates/tools4a-browser/src/orchestrator.rs` — replace the Phase 2 deferral with a `SocksTunnel` build + proxy injection path.
- `crates/tools4a-browser/src/orchestrator.rs` tests — replace `rejects_ssh_tunnel_with_phase2_message` with `accepts_ssh_tunnel_via_socks` (mock-based) and a new `errors_when_user_proxy_conflicts_with_ssh_tunnel`.
- `skills/browser-using/SKILL.md` — delete the "Phase 1 workaround" section, add a "Tunneling via SSH (SOCKS5)" section.
- `commands/browser.md` — update the tunnel-error guidance.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — mark Phase 2 done.

---

## Task 1: SOCKS5 codec module (pure protocol code)

**Files:**
- Create: `crates/tools4a-core/src/tunnel/socks/mod.rs`
- Create: `crates/tools4a-core/src/tunnel/socks/codec.rs`
- Modify: `crates/tools4a-core/src/tunnel/mod.rs`

Rationale for going codec-first: SOCKS5 is wire-level, deterministic, and 100% testable from bytes without any IO. Getting the codec correct + tested before wiring it to russh / tokio means the integration work later in this plan has a stable foundation.

- [ ] **Step 1: `crates/tools4a-core/src/tunnel/socks/mod.rs`**

```rust
//! SOCKS5 server building blocks. Used by `SocksTunnel` to listen
//! locally and forward each accepted connection through an SSH
//! session as a `direct-tcpip` channel. The codec sub-module is
//! pure (bytes in -> bytes out, no IO).

pub mod codec;
pub mod connector;
pub mod server;

pub use codec::{ReplyCode, Request, parse_greeting, parse_request, write_greeting_reply, write_request_reply};
pub use connector::{Connector, SshConnector};
pub use server::Socks5Server;
```

For this task, only `codec` exists — Tasks 2+3 add `connector` and `server`. Comment out or temporarily remove the `connector` / `server` lines until those tasks land. Use this final form when copying.

- [ ] **Step 2: `crates/tools4a-core/src/tunnel/socks/codec.rs`**

Implements the subset of [RFC 1928](https://www.rfc-editor.org/rfc/rfc1928) we need: method negotiation (greeting), CONNECT request, reply. No SOCKS5 auth (only method `0x00` no-auth).

```rust
//! SOCKS5 wire-format encoder/decoder. Pure functions over bytes —
//! no IO, no allocations beyond the returned Vec for replies.
//!
//! Scope: SOCKS5 (RFC 1928) handshake + CONNECT request/reply.
//! UDP ASSOCIATE / BIND are out of scope. Only the no-auth method
//! (0x00) is supported; this is fine because the SOCKS listener is
//! bound to 127.0.0.1.

use crate::{Error, Result};

pub const SOCKS5_VER: u8 = 0x05;
pub const METHOD_NO_AUTH: u8 = 0x00;
pub const METHOD_NO_ACCEPTABLE: u8 = 0xFF;

pub const CMD_CONNECT: u8 = 0x01;

pub const ATYP_IPV4: u8 = 0x01;
pub const ATYP_DOMAIN: u8 = 0x03;
pub const ATYP_IPV6: u8 = 0x04;

/// SOCKS5 reply codes (RFC 1928 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReplyCode {
    Succeeded = 0x00,
    GeneralFailure = 0x01,
    ConnectionNotAllowed = 0x02,
    NetworkUnreachable = 0x03,
    HostUnreachable = 0x04,
    ConnectionRefused = 0x05,
    TtlExpired = 0x06,
    CommandNotSupported = 0x07,
    AddressTypeNotSupported = 0x08,
}

/// Parsed CONNECT request from a SOCKS5 client.
#[derive(Debug, Clone)]
pub struct Request {
    /// Target host. For ATYP=Domain this is the raw bytes interpreted
    /// as UTF-8 (Chrome/agent-browser will send valid DNS names);
    /// for ATYP=IPv4/IPv6 this is the IP rendered as a string.
    pub host: String,
    pub port: u16,
    /// Raw ATYP byte echoed back in the reply.
    pub atyp: u8,
}

/// Parse the client greeting: `VER NMETHODS METHODS...`.
/// Returns Ok(()) if it advertises the no-auth method (the only one
/// we support); Err(Error::Service(...)) otherwise.
///
/// Caller-supplied buffer must contain at least the full greeting.
pub fn parse_greeting(buf: &[u8]) -> Result<()> {
    if buf.len() < 2 {
        return Err(Error::Service("SOCKS5 greeting too short".into()));
    }
    if buf[0] != SOCKS5_VER {
        return Err(Error::Service(format!(
            "SOCKS5: unsupported version 0x{:02x}",
            buf[0]
        )));
    }
    let n = buf[1] as usize;
    if buf.len() < 2 + n {
        return Err(Error::Service(
            "SOCKS5 greeting truncated (NMETHODS exceeds buffer)".into(),
        ));
    }
    if buf[2..2 + n].iter().any(|&m| m == METHOD_NO_AUTH) {
        Ok(())
    } else {
        Err(Error::Service(
            "SOCKS5 client did not offer no-auth (0x00) method".into(),
        ))
    }
}

/// Encode the server's method-selection reply: `VER METHOD`.
/// On success returns the no-auth selection; on rejection returns
/// 0xFF and the caller is expected to close the connection.
pub fn write_greeting_reply(accepted: bool) -> [u8; 2] {
    [
        SOCKS5_VER,
        if accepted { METHOD_NO_AUTH } else { METHOD_NO_ACCEPTABLE },
    ]
}

/// Parse a SOCKS5 CONNECT request:
/// `VER CMD RSV ATYP DST.ADDR DST.PORT`.
///
/// Returns the parsed `Request` plus the number of bytes consumed
/// (caller may have over-read into the next message — for SOCKS5
/// CONNECT this is moot, but the convention helps testing).
pub fn parse_request(buf: &[u8]) -> Result<(Request, usize)> {
    if buf.len() < 4 {
        return Err(Error::Service("SOCKS5 request too short".into()));
    }
    if buf[0] != SOCKS5_VER {
        return Err(Error::Service(format!(
            "SOCKS5: unsupported version in request 0x{:02x}",
            buf[0]
        )));
    }
    if buf[1] != CMD_CONNECT {
        return Err(Error::Service(format!(
            "SOCKS5: unsupported command 0x{:02x} (only CONNECT)",
            buf[1]
        )));
    }
    // buf[2] is RSV — ignored.
    let atyp = buf[3];
    let (host, addr_len) = match atyp {
        ATYP_IPV4 => {
            if buf.len() < 4 + 4 + 2 {
                return Err(Error::Service("SOCKS5 IPv4 request truncated".into()));
            }
            let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
            (ip.to_string(), 4)
        }
        ATYP_DOMAIN => {
            if buf.len() < 5 {
                return Err(Error::Service("SOCKS5 Domain request truncated (no len)".into()));
            }
            let dlen = buf[4] as usize;
            if buf.len() < 5 + dlen + 2 {
                return Err(Error::Service("SOCKS5 Domain request truncated".into()));
            }
            let name = std::str::from_utf8(&buf[5..5 + dlen])
                .map_err(|_| Error::Service("SOCKS5 Domain not UTF-8".into()))?
                .to_string();
            (name, 1 + dlen) // leading length byte + name
        }
        ATYP_IPV6 => {
            if buf.len() < 4 + 16 + 2 {
                return Err(Error::Service("SOCKS5 IPv6 request truncated".into()));
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&buf[4..20]);
            (std::net::Ipv6Addr::from(octets).to_string(), 16)
        }
        other => {
            return Err(Error::Service(format!(
                "SOCKS5: unsupported ATYP 0x{other:02x}"
            )));
        }
    };
    let port_offset = 4 + addr_len;
    let port = u16::from_be_bytes([buf[port_offset], buf[port_offset + 1]]);
    let total = port_offset + 2;
    Ok((Request { host, port, atyp }, total))
}

/// Build the SOCKS5 reply:
/// `VER REP RSV ATYP BND.ADDR BND.PORT`.
///
/// BND.ADDR / BND.PORT carry the server-side bound endpoint of the
/// outgoing socket — for direct-tcpip channels we don't have a
/// stable local address to report, so we echo back the client's
/// requested ATYP/host:port (RFC permits 0.0.0.0:0; echoing back is
/// equally valid and slightly more informative for tcpdump
/// debugging). Clients ignore these bytes for CONNECT.
pub fn write_request_reply(rep: ReplyCode, atyp_echo: u8, host: &str, port: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(22);
    out.push(SOCKS5_VER);
    out.push(rep as u8);
    out.push(0x00); // RSV
    match atyp_echo {
        ATYP_IPV4 => {
            out.push(ATYP_IPV4);
            let ip: std::net::Ipv4Addr = host.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
            out.extend_from_slice(&ip.octets());
        }
        ATYP_DOMAIN => {
            out.push(ATYP_DOMAIN);
            let bytes = host.as_bytes();
            // RFC limit: domain length <= 255; clamp defensively.
            let len = bytes.len().min(255);
            out.push(len as u8);
            out.extend_from_slice(&bytes[..len]);
        }
        ATYP_IPV6 => {
            out.push(ATYP_IPV6);
            let ip: std::net::Ipv6Addr =
                host.parse().unwrap_or(std::net::Ipv6Addr::UNSPECIFIED);
            out.extend_from_slice(&ip.octets());
        }
        _ => {
            // Fallback: pretend it was IPv4 0.0.0.0.
            out.push(ATYP_IPV4);
            out.extend_from_slice(&[0, 0, 0, 0]);
        }
    }
    out.extend_from_slice(&port.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_no_auth_accepted() {
        // VER=5, NMETHODS=1, METHOD=0x00
        parse_greeting(&[0x05, 0x01, 0x00]).unwrap();
    }

    #[test]
    fn greeting_no_auth_among_many() {
        parse_greeting(&[0x05, 0x03, 0x02, 0x00, 0x01]).unwrap();
    }

    #[test]
    fn greeting_rejects_when_no_auth_missing() {
        let err = parse_greeting(&[0x05, 0x02, 0x02, 0x01]).unwrap_err();
        match err {
            Error::Service(m) => assert!(m.contains("no-auth")),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn greeting_rejects_wrong_version() {
        let err = parse_greeting(&[0x04, 0x01, 0x00]).unwrap_err();
        match err {
            Error::Service(m) => assert!(m.contains("version")),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn greeting_rejects_truncated() {
        parse_greeting(&[0x05]).unwrap_err();
        parse_greeting(&[0x05, 0x02, 0x00]).unwrap_err();
    }

    #[test]
    fn greeting_reply_encodes_accept_and_reject() {
        assert_eq!(write_greeting_reply(true), [0x05, 0x00]);
        assert_eq!(write_greeting_reply(false), [0x05, 0xFF]);
    }

    #[test]
    fn request_parses_ipv4() {
        // VER=5 CMD=CONNECT RSV ATYP=IPv4 1.2.3.4 :80
        let buf = [0x05, 0x01, 0x00, 0x01, 1, 2, 3, 4, 0x00, 0x50];
        let (req, n) = parse_request(&buf).unwrap();
        assert_eq!(req.host, "1.2.3.4");
        assert_eq!(req.port, 80);
        assert_eq!(req.atyp, ATYP_IPV4);
        assert_eq!(n, 10);
    }

    #[test]
    fn request_parses_domain() {
        // VER=5 CMD=CONNECT RSV ATYP=Domain LEN="example.com" :443
        let mut buf = vec![0x05, 0x01, 0x00, 0x03, 11];
        buf.extend_from_slice(b"example.com");
        buf.extend_from_slice(&443u16.to_be_bytes());
        let (req, n) = parse_request(&buf).unwrap();
        assert_eq!(req.host, "example.com");
        assert_eq!(req.port, 443);
        assert_eq!(req.atyp, ATYP_DOMAIN);
        assert_eq!(n, 4 + 1 + 11 + 2);
    }

    #[test]
    fn request_parses_ipv6() {
        let mut buf = vec![0x05, 0x01, 0x00, 0x04];
        buf.extend_from_slice(&[0u8; 16]); // ::
        buf.extend_from_slice(&80u16.to_be_bytes());
        let (req, _) = parse_request(&buf).unwrap();
        assert_eq!(req.host, "::");
        assert_eq!(req.port, 80);
    }

    #[test]
    fn request_rejects_non_connect() {
        let buf = [0x05, 0x02, 0x00, 0x01, 0, 0, 0, 0, 0, 80];
        let err = parse_request(&buf).unwrap_err();
        match err {
            Error::Service(m) => assert!(m.contains("CONNECT")),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn request_rejects_truncated_domain() {
        let buf = [0x05, 0x01, 0x00, 0x03, 0x05, b'a']; // says len=5, has 1
        parse_request(&buf).unwrap_err();
    }

    #[test]
    fn request_rejects_unsupported_atyp() {
        let buf = [0x05, 0x01, 0x00, 0x99];
        parse_request(&buf).unwrap_err();
    }

    #[test]
    fn reply_succeeded_domain_echo() {
        let out = write_request_reply(ReplyCode::Succeeded, ATYP_DOMAIN, "example.com", 443);
        assert_eq!(out[0], 0x05);
        assert_eq!(out[1], 0x00);
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], ATYP_DOMAIN);
        assert_eq!(out[4], 11);
        assert_eq!(&out[5..16], b"example.com");
        assert_eq!(&out[16..18], &443u16.to_be_bytes());
    }

    #[test]
    fn reply_failure_ipv4_zero() {
        let out = write_request_reply(ReplyCode::HostUnreachable, ATYP_IPV4, "0.0.0.0", 0);
        assert_eq!(out, vec![0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
    }
}
```

- [ ] **Step 3: Wire `socks` into `tunnel/mod.rs`**

Add `pub mod socks;` to `crates/tools4a-core/src/tunnel/mod.rs`. Don't re-export from there yet — keep the codec namespaced (`crate::tunnel::socks::codec::*`) until Task 5 lifts `SocksTunnel` to the top-level `Tunnel` re-export list.

- [ ] **Step 4: Verify**

```bash
cargo test --package tools4a-core --lib tunnel::socks::codec
```

Expected: 12 PASS (the codec unit tests).

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core/tunnel): SOCKS5 codec (greeting + CONNECT + reply)

Pure protocol module under tools4a_core::tunnel::socks::codec — bytes
in, bytes out, no IO. Implements the subset of RFC 1928 the browser
tool needs: method-negotiation greeting, CONNECT-only request parsing
(IPv4 / Domain / IPv6 ATYP), reply encoding with ATYP echo for tcpdump
readability.

12 unit tests cover the happy paths plus all the rejection cases
(wrong version, truncated, unsupported method/command/ATYP). This
codec is reused unchanged by the upcoming Connector / Socks5Server
modules and SocksTunnel; isolating it first keeps the russh-coupled
code small.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `Connector` trait + bridging glue

**Files:**
- Create: `crates/tools4a-core/src/tunnel/socks/connector.rs`
- Modify: `crates/tools4a-core/src/tunnel/socks/mod.rs`

The `Connector` trait isolates "how do we open a stream to (host, port)?" from "how do we speak SOCKS5 on the inbound socket?". This lets us:

- Mock-test the SOCKS server (Task 3) with a `MockConnector` that returns an in-memory duplex pipe.
- Wrap russh's channel-message protocol behind a normal `AsyncRead + AsyncWrite` so `tokio::io::copy_bidirectional` Just Works.

- [ ] **Step 1: Define the trait + `SshConnector` skeleton**

Create `crates/tools4a-core/src/tunnel/socks/connector.rs`:

```rust
//! Connector trait — abstracts "open a byte-stream to (host, port)"
//! so the SOCKS5 server can be tested without russh.
//!
//! `SshConnector` is the production impl: each `connect` call opens
//! a new `direct-tcpip` channel on the shared SSH session.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;

use crate::Result;

/// Async stream the SOCKS server bidirectionally copies bytes
/// between (inbound TCP socket <-> outbound `Stream`).
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

#[async_trait]
pub trait Connector: Send + Sync {
    /// Open a stream to `host:port` (DNS / IP / domain — the
    /// implementor decides how to resolve, if at all; SshConnector
    /// forwards the literal name through SSH `direct-tcpip` so the
    /// bastion does the resolution).
    async fn connect(&self, host: &str, port: u16) -> Result<Box<dyn Stream>>;
}

/// Production impl: open a russh `direct-tcpip` channel and wrap
/// it as an AsyncRead + AsyncWrite stream.
///
/// `session` is the last session in the SSH chain (the one whose
/// transport the channel rides on). It's wrapped in `Arc<Mutex>`
/// because russh's `channel_open_direct_tcpip` is `&mut self` —
/// concurrent SOCKS connections need exclusive access to the
/// `Handle` only for the brief moment of opening a channel; the
/// resulting channels are independent.
pub struct SshConnector {
    pub session: Arc<Mutex<russh::client::Handle<crate::session::AcceptAnyHostKey>>>,
}

#[async_trait]
impl Connector for SshConnector {
    async fn connect(&self, host: &str, port: u16) -> Result<Box<dyn Stream>> {
        let mut session = self.session.lock().await;
        let channel = session
            .channel_open_direct_tcpip(
                host,
                port as u32,
                "127.0.0.1", // originator address — irrelevant for our use
                0,           // originator port
            )
            .await
            .map_err(|e| crate::Error::Connection(format!("direct-tcpip open: {e}")))?;
        Ok(Box::new(crate::tunnel::socks::connector::channel_stream(channel)))
    }
}

/// Wrap a russh `Channel` in an AsyncRead + AsyncWrite adapter.
///
/// russh 0.46 represents a channel as `Channel<Msg>` with a
/// `make_writer()`/`make_reader()` (or equivalent) split. The exact
/// API name varies between russh versions; check the installed
/// version's docs. Conceptually:
///
/// - reading: await Channel msgs, extract Data payloads, yield bytes
/// - writing: call channel.data(bytes).await
///
/// If russh exposes a ready-made AsyncRead/AsyncWrite (e.g. via
/// `ChannelStream` or `into_stream()`), use that. Otherwise hand-roll
/// the adapter with a wrapping struct + `poll_read`/`poll_write`
/// forwarding to channel methods, queuing partial writes in a
/// VecDeque<u8>. ~80 LOC.
fn channel_stream(
    channel: russh::Channel<russh::client::Msg>,
) -> impl AsyncRead + AsyncWrite + Unpin + Send {
    // Implementation note: russh 0.46+ ships a `ChannelStream` /
    // `into_stream()` that returns AsyncRead+AsyncWrite directly.
    // If the version pinned in tools4a-core's Cargo.toml has that,
    // this body is a one-liner: `channel.into_stream()`.
    //
    // If it doesn't, write a small adapter struct here with:
    //   - rx: futures::stream::SelectAll-ish over channel.wait()
    //   - tx: tokio::io::AsyncWrite delegating to channel.data()
    //
    // The verification step below confirms which path applies.
    channel.into_stream()
}
```

> **Action item before committing:** confirm `russh::Channel<russh::client::Msg>::into_stream` exists in the pinned russh version (`grep "russh = " crates/tools4a-core/Cargo.toml` shows `russh = "0.46"`; check russh's CHANGELOG.md or docs.rs for `into_stream` in 0.46). If absent, hand-roll the adapter as described in the comment above — the next task's tests still apply, because they exercise the abstraction through `MockConnector` not `SshConnector`.

- [ ] **Step 2: Add `mod connector;` to `socks/mod.rs`**

Already in the final form from Task 1 Step 1. Verify the line is present.

- [ ] **Step 3: Smoke-test compiles**

```bash
cargo build --package tools4a-core
```

Expected: clean. No new test runs in this task — `Connector` is the abstraction; tests come in Task 3 once a `MockConnector` exists.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core/tunnel): Connector trait + SshConnector skeleton

Connector abstracts 'open a stream to (host, port)' so the SOCKS5
server can be unit-tested without russh. SshConnector wraps an
existing russh session Handle (shared via Arc<Mutex>) and opens a
direct-tcpip channel per call; the channel is wrapped as an
AsyncRead + AsyncWrite Stream that the SOCKS server then bidi-copies
against the inbound TCP socket.

Lock granularity: only the channel-open call holds the session
mutex; the resulting channels are independent and copy in parallel.

Channel-to-stream adapter uses russh::Channel::into_stream() if
available in the pinned version; otherwise a hand-rolled poll_read /
poll_write delegate (~80 LOC).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `Socks5Server::serve_one` (per-connection handler)

**Files:**
- Create: `crates/tools4a-core/src/tunnel/socks/server.rs`

This is where the codec + connector come together: one inbound TCP socket, one outbound stream from the connector, bidirectional copy in between.

- [ ] **Step 1: Write the server**

Create `crates/tools4a-core/src/tunnel/socks/server.rs`:

```rust
//! Per-connection SOCKS5 handler. Greedily reads the greeting +
//! request, dispatches the CONNECT through a `Connector`, then runs
//! `tokio::io::copy_bidirectional` until either half closes.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::tunnel::socks::codec::{
    ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6, ReplyCode, parse_greeting, parse_request,
    write_greeting_reply, write_request_reply,
};
use crate::tunnel::socks::connector::{Connector, Stream};
use crate::{Error, Result};

pub struct Socks5Server;

impl Socks5Server {
    /// Handle one accepted SOCKS5 client end-to-end.
    ///
    /// Reads the greeting + request from `inbound`, opens an outbound
    /// stream via `connector`, writes the appropriate reply back to
    /// `inbound`, and runs a bidirectional copy until either half EOFs.
    ///
    /// Returns Err only for protocol violations or connector failures
    /// that happen BEFORE the copy starts; copy-time errors (e.g. peer
    /// reset) are logged-but-not-bubbled because the connection is
    /// already past the point of useful recovery.
    pub async fn serve_one<S: Stream>(
        mut inbound: S,
        connector: Arc<dyn Connector>,
    ) -> Result<()> {
        // --- Greeting ---------------------------------------------------
        // Up to 2 + 255 bytes. We over-read by reading up to 257 in one shot.
        let mut greet_buf = [0u8; 257];
        let n = read_exact_at_least(&mut inbound, &mut greet_buf, 2).await?;
        // Re-parse with the actual nmethods to know if more bytes are needed.
        if n < 2 + greet_buf[1] as usize {
            // Read the remaining methods.
            let need = 2 + greet_buf[1] as usize - n;
            inbound
                .read_exact(&mut greet_buf[n..n + need])
                .await
                .map_err(|e| Error::Service(format!("SOCKS5 greeting read: {e}")))?;
        }
        let total = 2 + greet_buf[1] as usize;
        let greet_ok = parse_greeting(&greet_buf[..total]).is_ok();
        inbound
            .write_all(&write_greeting_reply(greet_ok))
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 greeting reply: {e}")))?;
        if !greet_ok {
            return Err(Error::Service("SOCKS5 client rejected".into()));
        }

        // --- Request ----------------------------------------------------
        // Bounded: 4 + (1 + 255) + 2 = 262 bytes max for Domain ATYP.
        let mut req_buf = [0u8; 262];
        // First, the fixed 4 bytes (VER CMD RSV ATYP) + at least 1 byte of
        // address (so the parser can branch on ATYP).
        inbound
            .read_exact(&mut req_buf[..5])
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 request header read: {e}")))?;
        let body_len = match req_buf[3] {
            ATYP_IPV4 => 3 + 2,           // 3 remaining IPv4 octets + port
            ATYP_DOMAIN => req_buf[4] as usize + 2, // name body + port (len byte already in [4])
            ATYP_IPV6 => 15 + 2,          // 15 remaining IPv6 octets + port
            other => {
                // Send a polite reply, then bail.
                let reply = write_request_reply(
                    ReplyCode::AddressTypeNotSupported,
                    ATYP_IPV4,
                    "0.0.0.0",
                    0,
                );
                let _ = inbound.write_all(&reply).await;
                return Err(Error::Service(format!(
                    "SOCKS5: unsupported ATYP 0x{other:02x}"
                )));
            }
        };
        inbound
            .read_exact(&mut req_buf[5..5 + body_len])
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 request body read: {e}")))?;
        let (req, _consumed) = parse_request(&req_buf[..5 + body_len])?;

        // --- Connect outbound -------------------------------------------
        let outbound = match connector.connect(&req.host, req.port).await {
            Ok(s) => s,
            Err(e) => {
                let reply = write_request_reply(
                    ReplyCode::HostUnreachable,
                    req.atyp,
                    &req.host,
                    req.port,
                );
                let _ = inbound.write_all(&reply).await;
                return Err(e);
            }
        };

        // --- Success reply ----------------------------------------------
        let reply = write_request_reply(ReplyCode::Succeeded, req.atyp, &req.host, req.port);
        inbound
            .write_all(&reply)
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 success reply: {e}")))?;

        // --- Bidirectional copy -----------------------------------------
        let mut outbound = outbound; // own it for the lifetime of the copy
        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;

        Ok(())
    }
}

/// Read at least `min` bytes into `buf` (may read more if the socket
/// has them buffered). Returns the actual count.
async fn read_exact_at_least<S: tokio::io::AsyncRead + Unpin>(
    s: &mut S,
    buf: &mut [u8],
    min: usize,
) -> Result<usize> {
    let mut filled = 0;
    while filled < min {
        let n = s
            .read(&mut buf[filled..])
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 read: {e}")))?;
        if n == 0 {
            return Err(Error::Service("SOCKS5 client closed prematurely".into()));
        }
        filled += n;
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use tokio::io::AsyncReadExt;

    /// In-memory Connector that records the requested (host, port) and
    /// returns a tokio duplex pipe whose other half the test can read
    /// from / write into.
    struct MockConnector {
        last_target: tokio::sync::Mutex<Option<(String, u16)>>,
        /// Bytes the mock will deliver to the SOCKS server (i.e. the
        /// "remote server -> SOCKS server" direction); the server then
        /// forwards them to its inbound socket.
        canned_reply: Vec<u8>,
        /// Captured bytes the SOCKS server sent toward the "remote".
        captured: Arc<tokio::sync::Mutex<Vec<u8>>>,
    }

    #[async_trait]
    impl Connector for MockConnector {
        async fn connect(&self, host: &str, port: u16) -> Result<Box<dyn Stream>> {
            *self.last_target.lock().await = Some((host.to_string(), port));
            let (server_side, mut peer_side) = tokio::io::duplex(8192);
            // Deliver the canned reply onto peer_side so the server reads it.
            let canned = self.canned_reply.clone();
            let captured = self.captured.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = peer_side.write_all(&canned).await;
                let mut buf = vec![0u8; 4096];
                while let Ok(n) = peer_side.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    captured.lock().await.extend_from_slice(&buf[..n]);
                }
            });
            Ok(Box::new(server_side))
        }
    }

    #[tokio::test]
    async fn happy_path_domain_target() {
        let (mut client, server) = tokio::io::duplex(8192);

        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let connector: Arc<dyn Connector> = Arc::new(MockConnector {
            last_target: tokio::sync::Mutex::new(None),
            canned_reply: b"HTTP/1.0 200 OK\r\n\r\nhello".to_vec(),
            captured: captured.clone(),
        });

        let connector_for_assert = connector.clone();
        let task = tokio::spawn(async move {
            Socks5Server::serve_one(server, connector).await
        });

        // Client sends greeting
        use tokio::io::AsyncWriteExt;
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        // Read greeting reply
        let mut g = [0u8; 2];
        client.read_exact(&mut g).await.unwrap();
        assert_eq!(g, [0x05, 0x00]);
        // Send CONNECT example.com:80
        let mut req = vec![0x05, 0x01, 0x00, 0x03, 11];
        req.extend_from_slice(b"example.com");
        req.extend_from_slice(&80u16.to_be_bytes());
        client.write_all(&req).await.unwrap();
        // Read CONNECT reply
        let mut head = [0u8; 4];
        client.read_exact(&mut head).await.unwrap();
        assert_eq!(head, [0x05, 0x00, 0x00, 0x03]);
        let mut len = [0u8; 1];
        client.read_exact(&mut len).await.unwrap();
        let mut name = vec![0u8; len[0] as usize];
        client.read_exact(&mut name).await.unwrap();
        let mut port = [0u8; 2];
        client.read_exact(&mut port).await.unwrap();
        assert_eq!(name, b"example.com");
        assert_eq!(u16::from_be_bytes(port), 80);
        // Send some payload toward "the remote"
        client.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
        // Read the canned response back
        let mut resp = [0u8; 25];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(&resp, b"HTTP/1.0 200 OK\r\n\r\nhello");

        // Closing the client side ends the copy loop.
        drop(client);
        task.await.unwrap().unwrap();

        // The connector was asked for example.com:80.
        let mc = connector_for_assert
            .as_ref()
            .downcast_ref::<MockConnector>()
            .unwrap();
        assert_eq!(
            *mc.last_target.lock().await,
            Some(("example.com".to_string(), 80))
        );
        // The bytes the SOCKS server sent through to "the remote".
        assert_eq!(captured.lock().await.as_slice(), b"GET / HTTP/1.0\r\n\r\n");
    }

    // Additional tests TBD by the implementing agent:
    //   - rejects greeting without no-auth method (assert 0x05 0xFF reply)
    //   - rejects non-CONNECT command
    //   - connector failure surfaces HostUnreachable reply
    //   - IPv4 ATYP happy path
}
```

> **Note on `downcast_ref`:** the test snippet above uses a downcast to inspect the MockConnector's recorded target. `Arc<dyn Connector>` is `dyn`, so this won't compile as-is without making `Connector: std::any::Any` or storing the MockConnector concretely. The simpler fix: drop the downcast and inspect `captured` instead (which is already `Arc<Mutex<Vec<u8>>>` held outside the trait object). Adjust before committing.

- [ ] **Step 2: Verify**

```bash
cargo test --package tools4a-core --lib tunnel::socks::server
```

Expected: 1+ PASS (`happy_path_domain_target` plus whichever extras the implementing agent fills in).

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(core/tunnel): Socks5Server::serve_one (per-conn handler)

End-to-end SOCKS5 handshake for one inbound connection:
greeting -> method-selection reply -> CONNECT request -> Connector
dispatch -> success reply -> tokio::io::copy_bidirectional until
either half EOFs. Failure paths send the right SOCKS5 error code
(HostUnreachable on connector error, AddressTypeNotSupported on
weird ATYP) before bailing.

Tests use a MockConnector with tokio::io::duplex to drive the
server through a full happy-path domain CONNECT without any russh
involvement.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `SocksTunnel: impl Tunnel`

**Files:**
- Create: `crates/tools4a-core/src/tunnel/socks_tunnel.rs`
- Modify: `crates/tools4a-core/src/tunnel/mod.rs`

Now we lift the SOCKS server into the existing `Tunnel` lifecycle: `establish()` binds + spawns the accept loop; `close()` shuts it down.

- [ ] **Step 1: Write `SocksTunnel`**

Create `crates/tools4a-core/src/tunnel/socks_tunnel.rs`:

```rust
//! SOCKS5 server over an SSH session chain. Each accepted SOCKS
//! connection opens a fresh russh `direct-tcpip` channel to the
//! requested (host, port); the bastion does the actual TCP connect
//! and DNS resolution.
//!
//! Lifecycle:
//! - `new()` records the SSH chain config (no IO yet).
//! - `establish()` builds the session chain, binds a localhost TCP
//!   listener on a random port, spawns the accept loop, and returns
//!   `127.0.0.1:<port>` as the TunnelEndpoint.
//! - `close()` aborts the accept loop and disconnects the session
//!   chain.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;

use crate::session::{AcceptAnyHostKey, build_session_chain};
use crate::tunnel::socks::connector::{SshConnector};
use crate::tunnel::socks::server::Socks5Server;
use crate::{Error, Result, Tunnel, TunnelEndpoint};

pub struct SocksTunnel {
    ssh_jumps: Vec<(String, u16)>,
    ssh_user: String,
    ssh_password: Option<String>,
    ssh_key_path: Option<std::path::PathBuf>,
    ssh_port: u16,

    /// Set on `establish()`, cleared on `close()`.
    state: Option<EstablishedState>,
}

struct EstablishedState {
    listener_addr: std::net::SocketAddr,
    accept_task: JoinHandle<()>,
    shutdown_tx: oneshot::Sender<()>,
    /// The SSH session handle the accept loop's connector still references.
    /// Held here so `close()` can disconnect it explicitly after stopping
    /// the loop.
    session: Arc<Mutex<russh::client::Handle<AcceptAnyHostKey>>>,
}

impl SocksTunnel {
    pub fn new(
        ssh_jumps: Vec<(String, u16)>,
        ssh_user: String,
        ssh_password: Option<String>,
        ssh_key_path: Option<std::path::PathBuf>,
        ssh_port: u16,
    ) -> Result<Self> {
        if ssh_jumps.is_empty() {
            return Err(Error::Config(
                "SocksTunnel: ssh_jumps must not be empty".into(),
            ));
        }
        Ok(Self {
            ssh_jumps,
            ssh_user,
            ssh_password,
            ssh_key_path,
            ssh_port,
            state: None,
        })
    }
}

#[async_trait]
impl Tunnel for SocksTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        if self.state.is_some() {
            return Err(Error::Connection(
                "SocksTunnel: already established".into(),
            ));
        }

        // 1. Build the SSH chain. Same helper SshTunnel uses.
        let session = build_session_chain(
            &self.ssh_jumps,
            &self.ssh_user,
            self.ssh_password.as_deref(),
            self.ssh_key_path.as_deref(),
            self.ssh_port,
        )
        .await
        .map_err(|e| Error::Connection(format!("SocksTunnel: ssh chain: {e}")))?;
        let session = Arc::new(Mutex::new(session));

        // 2. Bind a localhost listener on a random port.
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| Error::Connection(format!("SocksTunnel: bind: {e}")))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| Error::Connection(format!("SocksTunnel: local_addr: {e}")))?;

        // 3. Spawn the accept loop.
        let connector_session = session.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let accept_task = tokio::spawn(async move {
            let connector: Arc<dyn crate::tunnel::socks::connector::Connector> =
                Arc::new(SshConnector {
                    session: connector_session,
                });
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _peer)) => {
                                let c = connector.clone();
                                tokio::spawn(async move {
                                    if let Err(e) =
                                        Socks5Server::serve_one(stream, c).await
                                    {
                                        eprintln!("SocksTunnel: serve_one: {e}");
                                    }
                                });
                            }
                            Err(e) => {
                                eprintln!("SocksTunnel: accept: {e}");
                                // Don't spin on EMFILE etc; back off briefly.
                                tokio::time::sleep(
                                    std::time::Duration::from_millis(50),
                                ).await;
                            }
                        }
                    }
                }
            }
        });

        self.state = Some(EstablishedState {
            listener_addr: local_addr,
            accept_task,
            shutdown_tx,
            session,
        });

        Ok(TunnelEndpoint {
            host: local_addr.ip().to_string(),
            port: local_addr.port(),
        })
    }

    async fn close(&mut self) -> Result<()> {
        let Some(state) = self.state.take() else {
            return Ok(());
        };
        // Signal the accept loop. Even if the receiver was already dropped
        // (loop already exited for some other reason), this is fine.
        let _ = state.shutdown_tx.send(());
        // Wait for the accept loop. Per-conn tasks may outlive this; they
        // get torn down when their channels EOF naturally as the session
        // disconnects below.
        let _ = state.accept_task.await;
        // Disconnect the SSH session.
        let mut s = state.session.lock().await;
        let _ = s
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
        let _ = state.listener_addr; // silence unused
        Ok(())
    }
}

impl Drop for SocksTunnel {
    fn drop(&mut self) {
        // Best-effort. If the caller didn't call close(), abort the
        // accept loop here so we don't leak a tokio task. The session
        // will eventually disconnect on its own when the Arc count
        // reaches zero.
        if let Some(state) = self.state.take() {
            let _ = state.shutdown_tx.send(());
            state.accept_task.abort();
        }
    }
}
```

- [ ] **Step 2: Wire `socks_tunnel` into `tunnel/mod.rs`**

Add to `crates/tools4a-core/src/tunnel/mod.rs`:

```rust
pub mod socks;
pub mod socks_tunnel;

pub use socks_tunnel::SocksTunnel;
```

Then in `crates/tools4a-core/src/lib.rs`, re-export `SocksTunnel` next to the existing `SshTunnel` / `DirectTunnel` re-exports so downstream crates can `use tools4a_core::SocksTunnel`.

- [ ] **Step 3: Verify**

```bash
cargo build --package tools4a-core
cargo test --package tools4a-core
cargo clippy --all-targets -- -D warnings
```

Expected: clean; no new tests added at this level — driving SocksTunnel end-to-end requires a real SSH server (out of scope for unit tests). The Phase-2 happy path is verified by the orchestrator-level test in Task 5 (which can mock the tunnel) and the manual smoke at Task 8.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core/tunnel): SocksTunnel — SOCKS5 over an SSH chain

establish() builds the session chain (shared helper with SshTunnel),
binds 127.0.0.1:0, and spawns an accept loop that serves each
inbound conn via Socks5Server::serve_one with an SshConnector that
opens fresh direct-tcpip channels on the shared session. close()
signals the accept loop, awaits it, and disconnects the session.
Drop is a best-effort guard for callers that forget close().

Session handle is shared as Arc<Mutex<Handle>> across per-conn
tasks; the mutex is only held during the brief channel-open call,
so multiple concurrent SOCKS conns get independent channels and
copy in parallel.

No new tests at this level — SocksTunnel needs a real SSH server
to exercise. Orchestrator-level mock test lands in the next commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Wire `BrowserOrchestrator` to build a `SocksTunnel` for `tunnel=ssh`

**Files:**
- Modify: `crates/tools4a-browser/src/orchestrator.rs`
- Modify: `crates/tools4a-browser/src/orchestrator.rs` tests

This is where Phase 1's deferral goes away. The orchestrator now:

1. Builds a `SocksTunnel` from `TunnelConfig::Ssh`.
2. `establish()` to get `127.0.0.1:<rand>`.
3. Validates the user didn't also set `proxy` (would conflict).
4. Mutates the request: `proxy = Some(format!("socks5://127.0.0.1:<rand>"))`.
5. Dispatches into `execute(req)`.
6. `tunnel.close()` regardless of execute outcome.

- [ ] **Step 1: Rewrite the orchestrator**

Replace `crates/tools4a-browser/src/orchestrator.rs` with:

```rust
//! BrowserOrchestrator — `Service` impl for the browser tool.
//!
//! Tunnel behavior:
//!   - None / Direct: spawn agent-browser as-is.
//!   - Ssh: build a SocksTunnel, inject `--proxy socks5://<endpoint>`
//!          into the request, then run; tear the tunnel down on exit.

use async_trait::async_trait;
use tools4a_core::{
    Error, ExecutionResult, Result, Service, SocksTunnel, Tunnel, TunnelConfig,
};

use crate::execute::execute;
use crate::request::BrowserRequest;

pub struct BrowserOrchestrator;

#[async_trait]
impl Service for BrowserOrchestrator {
    type Request = BrowserRequest;

    async fn execute(
        mut req: Self::Request,
        tunnel: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        match tunnel {
            None | Some(TunnelConfig::Direct) => execute(req).await,
            Some(TunnelConfig::Ssh {
                ssh_jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
            }) => {
                if req.proxy.is_some() {
                    return Err(Error::Config(
                        "tunnel=ssh and an explicit `proxy` field conflict: \
                         tools4a injects `--proxy socks5://...` when ssh is set. \
                         Pick one — drop `proxy` and let tools4a do it, or use \
                         tunnel=direct + your own proxy."
                            .into(),
                    ));
                }

                let mut t = SocksTunnel::new(
                    ssh_jumps,
                    ssh_user,
                    ssh_password,
                    ssh_key_path.map(std::path::PathBuf::from),
                    ssh_port,
                )?;
                let endpoint = t.establish().await?;
                req.proxy = Some(format!("socks5://{}:{}", endpoint.host, endpoint.port));

                let result = execute(req).await;

                // Tear down regardless of outcome. Errors here don't
                // override the execute() result; they're logged for
                // operators but the call is already done.
                if let Err(e) = t.close().await {
                    eprintln!("BrowserOrchestrator: SocksTunnel close: {e}");
                }
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> BrowserRequest {
        BrowserRequest {
            subcommand: "snapshot".into(),
            args: Vec::new(),
            session: None,
            proxy: None,
            proxy_bypass: None,
            browser_args: None,
            bin: Some(std::path::PathBuf::from("/nonexistent/ab")),
        }
    }

    #[tokio::test]
    async fn errors_when_user_proxy_conflicts_with_ssh_tunnel() {
        let mut r = req();
        r.proxy = Some("socks5://example.com:1080".into());
        let err = BrowserOrchestrator::execute(
            r,
            Some(TunnelConfig::Ssh {
                ssh_jumps: vec![("bastion.example.com".to_string(), 22)],
                ssh_user: "admin".to_string(),
                ssh_password: None,
                ssh_key_path: None,
                ssh_port: 22,
            }),
        )
        .await
        .unwrap_err();
        match err {
            Error::Config(m) => {
                assert!(m.contains("conflict"), "got: {m}");
                assert!(m.contains("socks5"), "got: {m}");
            }
            other => panic!("got {other:?}"),
        }
    }

    // accepts_ssh_tunnel_via_socks: covered by manual smoke + Task 8.
    // The full happy-path requires a real SSH server to listen on (and a
    // real agent-browser to consume the proxy), which we don't spin up in
    // unit tests. The orchestrator's logic is straight-line code below the
    // proxy-conflict check; an end-to-end test would mostly exercise
    // SocksTunnel + SshConnector, not the orchestrator.
}
```

- [ ] **Step 2: Verify**

```bash
cargo test --package tools4a-browser
```

Expected: existing browser tests pass; `errors_when_user_proxy_conflicts_with_ssh_tunnel` PASS; the old `rejects_ssh_tunnel_with_phase2_message` test is REPLACED (its expectation was the Phase 1 deferral that we just deleted — delete the old test in the same commit).

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(browser): wire BrowserOrchestrator through SocksTunnel

Phase 2 of the browser tool: tunnel=ssh no longer returns
Error::Config. Instead the orchestrator builds a SocksTunnel from
the TunnelConfig::Ssh fields, establish()es it to get a local
endpoint, injects 'socks5://<endpoint>' into BrowserRequest.proxy,
then dispatches into execute(). Tunnel.close() runs on return
regardless of outcome; errors there are logged but don't override
the call's result.

Conflict guard: if the user set both tunnel=ssh AND proxy=..., we
return Error::Config explaining the conflict and asking them to
pick one. Silently overriding would mask a likely user mistake.

Old test rejects_ssh_tunnel_with_phase2_message removed (its
expectation — the Phase 1 deferral — no longer exists). Replaced
with errors_when_user_proxy_conflicts_with_ssh_tunnel. Full
happy-path verification deferred to the manual smoke in Task 8.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Update the browser-using skill + slash command

**Files:**
- Modify: `skills/browser-using/SKILL.md`
- Modify: `commands/browser.md`

The Phase 1 SKILL/command bake in the manual `ssh -D 1080` workaround. Phase 2 makes that obsolete; update the docs so the next reader doesn't take the long path.

- [ ] **Step 1: Skill**

In `skills/browser-using/SKILL.md`:

a) Replace the `## Tunneling to internal HTTPS (Phase 1 workaround)` section with:

```markdown
## Tunneling via SSH (SOCKS5)

Set `tunnel = "ssh"` plus the usual `ssh_jump` / `ssh_user` / etc.
fields and tools4a will:

1. Build an SSH session chain to the bastion(s) (same chain code the
   other six tools use).
2. Bind a SOCKS5 listener on `127.0.0.1:<random>` whose CONNECT
   requests open fresh `direct-tcpip` channels through the SSH chain
   — the bastion does the actual TCP connect + DNS resolution.
3. Inject `--proxy socks5://127.0.0.1:<random>` into the
   `agent-browser` invocation. Chrome/agent-browser routes ALL of
   the page's traffic (HTTP, HTTPS, sub-resources, websockets)
   through that proxy, so internal HTTPS services with valid certs
   work without any tools4a-side TLS handling.
4. Tear the tunnel down on exit.

If you ALSO set `proxy` explicitly while `tunnel = "ssh"`, that's a
config error (`Error::Config("conflict ...")`) — drop one or the
other.

The Phase 1 manual workaround (`ssh -D 1080` + `proxy =
"socks5://127.0.0.1:1080"`) still works in `tunnel = "direct"` mode
if you want to keep the SSH listener separately for some reason
(e.g. multiple tools sharing one bastion); but the inline form is
preferred.
```

b) Remove the "Phase 1 workaround" phrasing from any other section.

- [ ] **Step 2: Slash command**

In `commands/browser.md`, update the "When something fails" block:

Replace:
```markdown
- `Error::Config("tunnel=ssh is not supported ... Phase 1 ...")` -> the user asked for `--tunnel=ssh` for the browser. Phase 2 will handle this; workaround in the error message (run `ssh -D 1080` + `--proxy socks5://127.0.0.1:1080`).
```

With:
```markdown
- `Error::Config("conflict ...")` -> the user set BOTH `--tunnel=ssh` AND `--proxy ...`. tools4a injects the proxy automatically when ssh is set; drop one of the two.
- SSH chain errors (host unreachable, auth failure, etc.) -> use the **ssh-bastion-checklist** skill.
```

- [ ] **Step 3: Verify**

```bash
grep -n "Phase 1 workaround\|tunnel=ssh is not supported" skills/ commands/ -r || echo "all references gone"
```

Expected: `all references gone`.

- [ ] **Step 4: Commit**

```bash
git add skills/browser-using/SKILL.md commands/browser.md
git commit -m "docs(browser): update skill + slash command for Phase 2

The Phase 1 'run ssh -D 1080 yourself + --proxy' workaround is gone
now that tunnel=ssh is wired through SocksTunnel. Updated:

- browser-using skill: 'Tunneling to internal HTTPS (Phase 1
  workaround)' section replaced with 'Tunneling via SSH (SOCKS5)'
  describing the inline path (tunnel=ssh + ssh_jump -> tools4a
  injects --proxy socks5://...).
- /browser slash command: 'When something fails' now points at the
  conflict error instead of the Phase 1 deferral message.

The manual workaround still works (tunnel=direct + user-provided
proxy) and is noted briefly; the inline form is recommended.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: README / CLAUDE.md / AGENTS.md updates

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `AGENTS.md`

- [ ] **Step 1: README — Status**

Move `SSH tunnel for the browser tool (needs SOCKS5 routing; Phase 2)` from "Not yet implemented" to a sub-bullet under the existing Browser entry:

```markdown
- **Browser CLI mode** (`tools4a browser <SUBCOMMAND> [ARGS]...`) and `browser_exec` MCP tool — thin wrapper around the externally-installed [`agent-browser`](https://github.com/vercel-labs/agent-browser) binary. `--tunnel=ssh` works via a built-in SOCKS5 server over the SSH chain — set the standard `--ssh-jump` / `--ssh-user` flags and the browser reaches the bastion-side network.
```

- [ ] **Step 2: README — Usage example**

Update the Browser usage block:

````markdown
### Browser

```bash
# Pre-req: install agent-browser separately, e.g. `npm i -g agent-browser`.

# Direct
tools4a browser open https://example.com --session work
tools4a browser click "selector=#login" --session work

# Through an SSH bastion (Phase 2: tools4a runs a built-in SOCKS5 server)
tools4a --tunnel=ssh --ssh-jump=bastion.example.com --ssh-user=admin \
  browser open https://internal-app.local --session work
```
````

- [ ] **Step 3: CLAUDE.md / AGENTS.md — Phase boundaries**

Find the existing Phase 14 entry and replace its tunnel paragraph:

Before:
```markdown
SSH tunnel is **NOT supported in Phase 1** — `tunnel=ssh` returns `Error::Config` with an explicit Phase 2 deferral message ...
```

After:
```markdown
SSH tunnel uses a SOCKS5 routing strategy (different from the single-port `direct-tcpip` model the other six tools use): `tools4a-core::tunnel::SocksTunnel` binds a localhost listener, accepts SOCKS5 connections, opens a fresh `direct-tcpip` channel through the SSH session for each (the bastion does TCP connect + DNS), and `BrowserOrchestrator` injects `--proxy socks5://127.0.0.1:<rand>` into the agent-browser invocation. Each call's SOCKS listener is per-invocation and torn down on exit. Profile/YAML config for browser defaults is still deferred (same simplification as HTTP / SSH-direct).
```

- [ ] **Step 4: CLAUDE.md / AGENTS.md — Module map**

Add a row after the existing `tools4a-core` row (or extend the existing one's description) noting the new submodule:

```markdown
| `tools4a-core::tunnel::socks::*` + `SocksTunnel` | Phase 14 Phase 2 addition: SOCKS5 codec (`codec.rs`), `Connector` trait + `SshConnector` (`connector.rs`), `Socks5Server::serve_one` (`server.rs`), and `SocksTunnel` that drives them. Used by `BrowserOrchestrator` to give the browser tool SOCKS-shaped SSH routing. Reuses `build_session_chain` unchanged. |
```

- [ ] **Step 5: CLAUDE.md / AGENTS.md — Conventions**

Append:

```markdown
- **Tunnel shape is service-specific**: five services use single-port `direct-tcpip` via `SshTunnel` (mysql/pgsql/clickhouse/redis/mongo/http — the wire protocol talks to one TCP endpoint); ssh-exec uses the session chain directly with no tcp forwarding; browser uses SOCKS5 via `SocksTunnel` (the wire protocol is "a browser, which needs to reach many hosts"). `TunnelConfig::Ssh` is purely a *credentials + chain* spec — the orchestrator picks which `Tunnel` impl to build. If a future service needs a third shape (e.g. dynamic per-call host:port resolution), add a new `Tunnel` impl rather than overloading `TunnelConfig`.
```

- [ ] **Step 6: Verify CLAUDE.md and AGENTS.md still match (modulo cross-link)**

```bash
diff <(tail -n +5 CLAUDE.md) <(tail -n +5 AGENTS.md)
```

Expected: only the cross-link line + methodology trailer differ.

- [ ] **Step 7: Commit**

```bash
git add README.md CLAUDE.md AGENTS.md
git commit -m "docs: document Phase 14 Phase 2 browser SOCKS5 tunnel

- README: Browser status entry now says tunnel=ssh works via the
  built-in SOCKS5 server; usage example replaces the manual 'ssh -D'
  workaround with the inline tunnel=ssh form.
- CLAUDE.md / AGENTS.md: Phase 14 entry's tunnel paragraph rewritten
  to describe the SocksTunnel path; module map gains a row for the
  new tools4a-core::tunnel::socks::* submodule; conventions adds a
  'tunnel shape is service-specific' note covering the three shapes
  in play (direct-tcpip / session-chain-direct / SOCKS).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Manual end-to-end smoke

**Files:** none (operator-driven verification).

The integration is too coupled to a real SSH server and a real agent-browser daemon to land a CI test. This task is a checklist for the operator to run once before announcing Phase 2 done.

- [ ] **Step 1: Pre-flight**

```bash
agent-browser --version    # confirm agent-browser is installed
which ssh                  # confirm ssh client is on PATH
# Pick a reachable bastion that you have SSH key auth to, with
# access to an internal HTTP(S) service.
```

- [ ] **Step 2: Direct (regression — Phase 1 path)**

```bash
./target/release/tools4a browser open https://example.com --session smoke
./target/release/tools4a browser snapshot --session smoke
```

Expected: `exit_code = 0`, `stdout` carries a snapshot payload. No change from Phase 1.

- [ ] **Step 3: SSH tunnel (the new path)**

```bash
./target/release/tools4a \
    --tunnel=ssh \
    --ssh-jump=<bastion-host> \
    --ssh-user=<bastion-user> \
    browser open https://<internal-host>/ --session ssh-smoke
```

Expected:

- `exit_code = 0`.
- The remote service receives the request (verify via its access log: source IP should be the bastion, not your local IP).
- `stdout` row carries agent-browser's response.

Open `127.0.0.1:<rand>` does NOT remain bound after the call returns — verify with `ss -ltn | grep 127.0.0.1` before / during / after the call. The listener should exist during the call and be gone within a second of return.

- [ ] **Step 4: Conflict path**

```bash
./target/release/tools4a \
    --tunnel=ssh \
    --ssh-jump=<bastion> \
    --ssh-user=<user> \
    browser open https://example.com \
    --proxy socks5://127.0.0.1:9999
```

Expected: exit 1, stderr contains `conflict` and `socks5`.

- [ ] **Step 5: Multi-jump regression**

If the bastion-side service is only reachable through TWO jumps, the same call with `--ssh-jump=jump1,jump2` should still work — `SocksTunnel` reuses `build_session_chain` unchanged.

```bash
./target/release/tools4a \
    --tunnel=ssh \
    --ssh-jump=<jump1>,<jump2> \
    --ssh-user=<user> \
    browser snapshot --session multi-hop
```

Expected: same shape as Step 3.

- [ ] **Step 6: Document the smoke result**

If anything fails, file a bug against Phase 2 with the offending command + a brief description of the symptom before merging. Don't paper over with retries.

---

## Summary

After Phase 14 Phase 2:

- `tools4a-core::tunnel::socks::*` ships a small, self-contained SOCKS5 server (codec + connector trait + per-conn handler) — usable in isolation if a future tool also wants SOCKS-shaped routing.
- `SocksTunnel` implements `Tunnel` end-to-end: bind, accept-loop, per-conn `direct-tcpip` channel, clean shutdown.
- `BrowserOrchestrator` no longer rejects `tunnel=ssh`; it builds a `SocksTunnel`, injects `socks5://...` into `BrowserRequest.proxy`, runs, tears down.
- Conflict path (`tunnel=ssh` + user `proxy`) returns `Error::Config` instead of silently overriding.
- Docs, skill, and slash command updated; the Phase 1 manual workaround is no longer the preferred path.

**No new external dependencies.** russh + tokio + async-trait — all already in `tools4a-core`.

**No changes to the other six tools.** `SshTunnel` (single-port `direct-tcpip`) continues to serve mysql/pgsql/clickhouse/redis/mongo/http unchanged. SOCKS is purely the browser's shape.

**Deferred (re-open if/when there's demand):**
- SOCKS4 / SOCKS4a, SOCKS5 auth, UDP ASSOCIATE / BIND.
- Per-call SOCKS connection limit / quotas / rate-limiting.
- Reusing a long-lived `SocksTunnel` across multiple `browser_exec` calls in the same MCP session (currently each call gets a fresh tunnel — fine because Chrome reuses HTTP connections inside one agent-browser daemon anyway).
- Profile/YAML config for browser defaults (still Phase 1's deferral; revisit only if a real user asks).
- An integration test using a synthetic SSH server (e.g. via russh's server crate) — would catch regressions but adds significant CI surface; the manual smoke is the contract for now.
