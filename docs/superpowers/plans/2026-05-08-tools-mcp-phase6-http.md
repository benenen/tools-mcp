# Tools MCP Phase 6: HTTP Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add HTTP request support — `tools-mcp http GET https://...` CLI subcommand and `http_exec` MCP tool that issue arbitrary HTTP/HTTPS requests, optionally routed through an SSH tunnel for accessing internal-network HTTP services.

**Architecture:** New `tools-mcp-http` lib crate (reqwest 0.12 + rustls), bin orchestrator `core::http::execute(req, tunnel_config)`, CLI subcommand `http <METHOD> <URL>` with curl-style flags, MCP `http_exec` tool. HTTP doesn't fit the `Connection` trait pattern (no persistent session), so the lib only uses `Tunnel` for routing — when a tunnel is set, reqwest's `resolve(host, addr)` override points the URL host at the tunnel's local listener while preserving Host header / TLS SNI / cert verification. **Phase 6 deliberately defers Profile/YAML support for HTTP** — only CLI flags + global `--tunnel`/`--ssh-*` apply. Output is a flat `field`/`value` `ExecutionResult` (status + headers + body).

**Tech Stack:** [`reqwest`](https://crates.io/crates/reqwest) 0.12 with `rustls-tls` (no OpenSSL dep) + `gzip` + `brotli` features; reuses existing `tools-mcp-core`, `DirectTunnel`, `SshTunnel`.

**Out of scope (Phase 7+):**
- Profile/YAML config for HTTP (`base_url`, default headers, default bearer token).
- Cookie jars, redirects beyond reqwest's default 10-redirect cap.
- Streaming responses (we read the whole body).
- WebSocket / SSE.
- Per-request retries.
- mTLS client certs.

---

## File Structure

**New:**
- `crates/tools-mcp-http/Cargo.toml` — reqwest with rustls + tools-mcp-core + async-trait.
- `crates/tools-mcp-http/src/lib.rs` — re-exports.
- `crates/tools-mcp-http/src/request.rs` — `HttpRequestSpec` (method/url/headers/body/insecure) + `HttpAuth` enum.
- `crates/tools-mcp-http/src/executor.rs` — `HttpExecutor::run(client, req)` + response → `ExecutionResult` mapping.
- `crates/tools-mcp-http/src/execute.rs` — `execute(tunnel, url_host, url_port, req) -> ExecutionResult` entry.

**Modified:**
- `Cargo.toml` (workspace) — add `crates/tools-mcp-http` to `members`; add `tools-mcp-http = { path = "crates/tools-mcp-http" }` to bin `[dependencies]`.
- `src/config/types.rs` — add `Http` variant to `ServiceType` enum (kept consistent for future profile support, even though Phase 6 doesn't use it).
- `src/core/mod.rs` — `pub mod http;`.
- `src/core/http.rs` — orchestrator `execute(HttpRequestSpec, Option<TunnelConfig>) -> ExecutionResult`.
- `src/cli/args.rs` — add `Commands::Http { method, url, header (Vec), data, data_file, json, bearer, basic, insecure, include_headers }`.
- `src/cli/handler.rs` — handle the new variant; add `execute_http` wrapper.
- `src/mcp/tools.rs` — add `HttpExecParams` + `http_exec(params)` entry.
- `src/mcp/server.rs` — register `#[tool] http_exec`.
- `commands/http.md` — new slash command.
- `skills/http-using/SKILL.md` — new skill.
- `README.md`, `CLAUDE.md`, `AGENTS.md` — document Phase 6.

---

## Task 1: Add `ServiceType::Http` + bootstrap empty `tools-mcp-http` crate

**Files:**
- Modify: `src/config/types.rs` (add `Http` variant to `ServiceType`)
- Modify: `Cargo.toml` (workspace `members` + bin `[dependencies]`)
- Create: `crates/tools-mcp-http/Cargo.toml`
- Create: `crates/tools-mcp-http/src/lib.rs` (placeholder)

- [ ] **Step 1: Add `Http` variant to `ServiceType`**

In `src/config/types.rs`, the existing `ServiceType` enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Mysql,
    Redis,
    Ssh,
}
```

Append a new variant:

```rust
    Http,
```

Also, the `impl FromStr for ServiceType` block has matching string arms. Add `"http" => Ok(ServiceType::Http),` next to the existing `mysql`/`redis`/`ssh` arms.

- [ ] **Step 2: Create `crates/tools-mcp-http/Cargo.toml`**

```toml
[package]
name = "tools-mcp-http"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "gzip", "brotli", "stream"] }
tools-mcp-core = { path = "../tools-mcp-core" }

[dev-dependencies]
tokio = { version = "1.40", features = ["macros", "rt-multi-thread"] }
```

`default-features = false` avoids reqwest pulling in native-tls (which requires OpenSSL on Linux). `rustls-tls` is the pure-Rust replacement.

- [ ] **Step 3: Create `crates/tools-mcp-http/src/lib.rs`**

```rust
//! HTTP request execution, layered on `tools-mcp-core` and (optionally) `Tunnel`.
```

- [ ] **Step 4: Wire workspace members and bin dep**

In root `Cargo.toml`:

a) Update `[workspace] members`:

```toml
[workspace]
resolver = "3"
members = [
    "crates/tools-mcp-core",
    "crates/tools-mcp-http",
    "crates/tools-mcp-mysql",
    "crates/tools-mcp-redis",
]
```

b) Update bin `[dependencies]` — add (alphabetical, between `tools-mcp-core` and `tools-mcp-mysql`):

```toml
tools-mcp-http = { path = "crates/tools-mcp-http" }
```

- [ ] **Step 5: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo test`
Expected: prior count passes (no new tests yet).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(http): add ServiceType::Http and tools-mcp-http stub

- ServiceType gains an Http variant (FromStr accepts \"http\");
  Phase 6 doesn't use it via Profile/YAML yet, but keeps the
  ServiceType enum consistent for future profile support.
- New empty crate crates/tools-mcp-http with reqwest 0.12 (rustls-tls
  + gzip + brotli + stream features) and tools-mcp-core deps.
- Workspace members updated; bin gains tools-mcp-http dep.

cargo test still passes; no behavior change.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `HttpRequestSpec` + `HttpAuth` types + Cargo.toml bookkeeping

**Files:**
- Create: `crates/tools-mcp-http/src/request.rs`
- Modify: `crates/tools-mcp-http/src/lib.rs`

- [ ] **Step 1: Define request types**

Create `crates/tools-mcp-http/src/request.rs`:

```rust
//! HTTP request input shape — independent of any caller (CLI, MCP, future tests).

/// Authentication scheme to apply to the outgoing request.
#[derive(Debug, Clone)]
pub enum HttpAuth {
    None,
    /// `Authorization: Bearer <token>`.
    Bearer(String),
    /// `Authorization: Basic <base64(user:pass)>`.
    Basic { user: String, password: String },
}

/// Resolved HTTP request to execute. Caller (CLI handler / MCP tool) builds
/// this from the user's flags / JSON params; the lib doesn't care where the
/// fields came from.
#[derive(Debug, Clone)]
pub struct HttpRequestSpec {
    /// Uppercased method name (GET / POST / PUT / DELETE / PATCH / HEAD).
    pub method: String,
    /// Full request URL including scheme + host + path + query.
    pub url: String,
    /// Extra headers as (name, value) pairs. Stored in insertion order.
    pub headers: Vec<(String, String)>,
    /// Optional request body. Already-encoded bytes; the lib doesn't transform it.
    pub body: Option<Vec<u8>>,
    /// Authentication scheme.
    pub auth: HttpAuth,
    /// If true, accept invalid TLS certs (e.g. self-signed). Default: false.
    pub insecure: bool,
}
```

- [ ] **Step 2: Update `crates/tools-mcp-http/src/lib.rs`**

```rust
//! HTTP request execution, layered on `tools-mcp-core` and (optionally) `Tunnel`.

pub mod request;

pub use request::{HttpAuth, HttpRequestSpec};
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo test`
Expected: prior count passes (no new tests added in this task — the types are too small to test in isolation; subsequent tasks exercise them via the executor).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(http): HttpRequestSpec + HttpAuth input types

Plain data types describing the HTTP request to execute. Caller
(CLI handler / MCP tool) builds these from their respective input
shapes; the lib doesn't care where fields came from.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `HttpExecutor::run` + response → `ExecutionResult` mapping

**Files:**
- Create: `crates/tools-mcp-http/src/executor.rs`
- Modify: `crates/tools-mcp-http/src/lib.rs`

- [ ] **Step 1: Write executor + response mapping**

Create `crates/tools-mcp-http/src/executor.rs`:

```rust
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use reqwest::{Client, Method};
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::request::{HttpAuth, HttpRequestSpec};

pub struct HttpExecutor;

impl HttpExecutor {
    /// Send the request through `client` and map the response into an
    /// `ExecutionResult` with one row per { status, header.*, body }.
    pub async fn run(client: &Client, req: HttpRequestSpec) -> Result<ExecutionResult> {
        let method = parse_method(&req.method)?;
        let mut builder = client.request(method, &req.url);

        // Headers
        let mut header_map = HeaderMap::new();
        for (name, value) in &req.headers {
            let h_name: HeaderName = name.parse().map_err(|e| {
                Error::Config(format!("invalid header name '{name}': {e}"))
            })?;
            let h_value: HeaderValue = value.parse().map_err(|e| {
                Error::Config(format!("invalid header value for '{name}': {e}"))
            })?;
            header_map.append(h_name, h_value);
        }

        // Auth
        match &req.auth {
            HttpAuth::None => {}
            HttpAuth::Bearer(token) => {
                let val: HeaderValue = format!("Bearer {token}")
                    .parse()
                    .map_err(|e| Error::Config(format!("invalid bearer token: {e}")))?;
                header_map.insert(AUTHORIZATION, val);
            }
            HttpAuth::Basic { user, password } => {
                builder = builder.basic_auth(user, Some(password));
            }
        }

        builder = builder.headers(header_map);

        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        let response = builder
            .send()
            .await
            .map_err(|e: reqwest::Error| Error::Service(format!("HTTP: {e}")))?;

        Ok(response_to_result(response).await?)
    }
}

fn parse_method(s: &str) -> Result<Method> {
    Method::from_bytes(s.to_uppercase().as_bytes())
        .map_err(|e| Error::Config(format!("invalid HTTP method '{s}': {e}")))
}

async fn response_to_result(response: reqwest::Response) -> Result<ExecutionResult> {
    let status = response.status();
    let status_line = format!(
        "{} {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );

    // Snapshot headers before consuming the response for the body.
    let mut header_rows: Vec<(String, String)> = Vec::new();
    for (name, value) in response.headers().iter() {
        let v = value.to_str().unwrap_or("<non-utf8 header value>").to_string();
        header_rows.push((format!("header.{name}"), v));
    }

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e: reqwest::Error| Error::Service(format!("HTTP body: {e}")))?;
    let body_cell = match std::str::from_utf8(&body_bytes) {
        Ok(text) => text.to_string(),
        Err(_) => format!("<{} bytes (non-UTF-8 body)>", body_bytes.len()),
    };

    let mut rows: Vec<Vec<String>> = Vec::with_capacity(2 + header_rows.len() + 1);
    rows.push(vec!["status_code".to_string(), status.as_u16().to_string()]);
    rows.push(vec!["status".to_string(), status_line]);
    for (name, value) in header_rows {
        rows.push(vec![name, value]);
    }
    rows.push(vec!["body".to_string(), body_cell]);

    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        rows,
        affected,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_method_uppercases() {
        assert_eq!(parse_method("get").unwrap(), Method::GET);
        assert_eq!(parse_method("POST").unwrap(), Method::POST);
        assert_eq!(parse_method("PaTcH").unwrap(), Method::PATCH);
    }

    #[test]
    fn test_parse_method_rejects_garbage() {
        let err = parse_method("not a method").unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("invalid HTTP method")));
    }
}
```

- [ ] **Step 2: Update `crates/tools-mcp-http/src/lib.rs`**

```rust
//! HTTP request execution, layered on `tools-mcp-core` and (optionally) `Tunnel`.

pub mod executor;
pub mod request;

pub use executor::HttpExecutor;
pub use request::{HttpAuth, HttpRequestSpec};
```

- [ ] **Step 3: Verify**

Run: `cargo test --package tools-mcp-http`
Expected: 2 PASS (the two `parse_method` tests).

Run: `cargo test`
Expected: prior count + 2 new = pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(http): HttpExecutor + response → ExecutionResult mapping

HttpExecutor::run builds a reqwest request from HttpRequestSpec,
applies headers + auth (Bearer / Basic / None), sends, then maps the
response to a flat field/value ExecutionResult: status_code, status,
header.* (one row per header), body. Body is UTF-8 if possible,
otherwise rendered as <N bytes (non-UTF-8 body)>.

2 unit tests cover the method-string parser.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `execute(tunnel, url_host, url_port, req)` entry function

**Files:**
- Create: `crates/tools-mcp-http/src/execute.rs`
- Modify: `crates/tools-mcp-http/src/lib.rs`

- [ ] **Step 1: Write the entry function**

Create `crates/tools-mcp-http/src/execute.rs`:

```rust
//! Top-level entry: build a reqwest client (with tunnel resolve override
//! when needed), send a single HTTP request, return the structured result.

use std::net::SocketAddr;
use std::str::FromStr;
use tools_mcp_core::{Error, ExecutionResult, Result, Tunnel};

use crate::executor::HttpExecutor;
use crate::request::HttpRequestSpec;

/// Send a single HTTP request through `tunnel`. The tunnel's endpoint is
/// only consulted to override DNS for the URL's host — when the endpoint
/// matches the URL host:port (i.e. DirectTunnel returning the same address),
/// reqwest performs a normal DNS lookup. When the tunnel rewrote the address
/// (i.e. SshTunnel returning 127.0.0.1:<local-port>), `resolve()` points the
/// URL host at the tunnel's local listener while preserving Host header,
/// TLS SNI, and cert verification against the original hostname.
pub async fn execute(
    mut tunnel: Box<dyn Tunnel>,
    url_host: String,
    url_port: u16,
    req: HttpRequestSpec,
) -> Result<ExecutionResult> {
    let endpoint = tunnel.establish().await?;

    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(req.insecure)
        .user_agent(concat!("tools-mcp/", env!("CARGO_PKG_VERSION")));

    let needs_resolve = endpoint.host != url_host || endpoint.port != url_port;
    if needs_resolve {
        let addr_str = format!("{}:{}", endpoint.host, endpoint.port);
        let addr = SocketAddr::from_str(&addr_str).map_err(|e| {
            Error::Connection(format!(
                "tunnel endpoint {addr_str} is not a SocketAddr (need IP:port for DNS override): {e}"
            ))
        })?;
        builder = builder.resolve(&url_host, addr);
    }

    let client = builder
        .build()
        .map_err(|e: reqwest::Error| Error::Service(format!("HTTP client init: {e}")))?;

    let result = HttpExecutor::run(&client, req).await;

    let _ = tunnel.close().await;
    result
}
```

- [ ] **Step 2: Update `crates/tools-mcp-http/src/lib.rs`**

```rust
//! HTTP request execution, layered on `tools-mcp-core` and (optionally) `Tunnel`.

pub mod execute;
pub mod executor;
pub mod request;

pub use execute::execute;
pub use executor::HttpExecutor;
pub use request::{HttpAuth, HttpRequestSpec};
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo test`
Expected: prior + 2 (the executor tests; this task adds no new tests — `execute` requires a real network endpoint and we don't run integration tests at the lib level).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(http): execute(tunnel, url_host, url_port, req) entry

Builds a reqwest::Client whose DNS resolves the URL's host to the
tunnel's local endpoint (when the tunnel rewrote the address) or uses
normal DNS (when DirectTunnel passed through the address unchanged).
TLS SNI / Host header / cert verification all keep using the original
URL host, so HTTPS through SSH tunnels works without special TLS
config.

Sets a tools-mcp/<version> User-Agent and honors the spec's insecure
flag (danger_accept_invalid_certs for self-signed targets). The
tunnel is closed before the function returns regardless of outcome.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Bin orchestrator `core::http::execute`

**Files:**
- Create: `src/core/http.rs`
- Modify: `src/core/mod.rs`

- [ ] **Step 1: Write the orchestrator**

Create `src/core/http.rs`:

```rust
//! Orchestrator: take an HttpRequestSpec + an optional TunnelConfig,
//! parse the URL, build the right tunnel, dispatch into tools_mcp_http.
//! CLI handler and MCP `http_exec` tool both delegate here so both
//! presentation layers go through identical request + tunnel teardown.

use crate::config::TunnelConfig;
use crate::tunnel::{DirectTunnel, SshTunnel};
use tools_mcp_core::{Error, ExecutionResult, Result, Tunnel};
use tools_mcp_http::{HttpRequestSpec, execute as http_execute};

pub async fn execute(
    req: HttpRequestSpec,
    tunnel_config: Option<TunnelConfig>,
) -> Result<ExecutionResult> {
    let parsed = reqwest::Url::parse(&req.url)
        .map_err(|e| Error::Config(format!("invalid URL '{}': {e}", req.url)))?;
    let url_host = parsed
        .host_str()
        .ok_or_else(|| Error::Config(format!("URL '{}' has no host", req.url)))?
        .to_string();
    let url_port = parsed.port_or_known_default().ok_or_else(|| {
        Error::Config(format!(
            "URL '{}' uses an unsupported scheme '{}' (need http/https)",
            req.url,
            parsed.scheme()
        ))
    })?;

    let tunnel: Box<dyn Tunnel> = match tunnel_config {
        None | Some(TunnelConfig::Direct) => {
            Box::new(DirectTunnel::new(url_host.clone(), url_port))
        }
        Some(TunnelConfig::Ssh {
            ssh_jumps,
            ssh_user,
            ssh_password,
            ssh_key_path,
            ssh_port,
        }) => {
            let key_path = ssh_key_path.map(std::path::PathBuf::from);
            Box::new(SshTunnel::new(
                ssh_jumps,
                ssh_user,
                ssh_password,
                key_path,
                ssh_port,
                url_host.clone(),
                url_port,
            )?)
        }
    };

    http_execute(tunnel, url_host, url_port, req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools_mcp_http::HttpAuth;

    fn empty_req(url: &str) -> HttpRequestSpec {
        HttpRequestSpec {
            method: "GET".to_string(),
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
            auth: HttpAuth::None,
            insecure: false,
        }
    }

    #[tokio::test]
    async fn test_execute_errors_on_invalid_url() {
        let err = execute(empty_req("not a url"), None).await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("invalid URL")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_missing_host() {
        // file:// is parseable but has no host.
        let err = execute(empty_req("file:///tmp/x"), None).await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("no host")));
    }

    #[tokio::test]
    async fn test_execute_errors_on_unsupported_scheme() {
        let err = execute(empty_req("ftp://example.com/x"), None).await.unwrap_err();
        assert!(matches!(err, Error::Config(msg) if msg.contains("unsupported scheme")));
    }
}
```

Note: this orchestrator pulls `reqwest` at the bin level for URL parsing. The bin already has it transitively through `tools-mcp-http`. **If `cargo build` complains that the bin can't see `reqwest`**, add `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }` to the bin's `[dependencies]` in the root `Cargo.toml`. Otherwise, leave the bin without it.

- [ ] **Step 2: Wire `core::http` into `src/core/mod.rs`**

Update `src/core/mod.rs` from:

```rust
pub mod mysql;
pub mod redis;
```

to:

```rust
pub mod http;
pub mod mysql;
pub mod redis;
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean. If the bin can't see `reqwest::Url`, add `reqwest` to the bin deps as noted above and rebuild.

Run: `cargo test test_execute_errors_on_invalid_url test_execute_errors_on_missing_host test_execute_errors_on_unsupported_scheme`
Expected: 3 PASS.

Run: `cargo test`
Expected: prior + 3 new = pass.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core): add core::http::execute orchestrator

Symmetric to core::mysql/redis::execute but takes (HttpRequestSpec,
Option<TunnelConfig>) instead of a Config — Phase 6 deliberately
defers Profile/YAML support for HTTP, so there's no 3-layer merge to
apply here. Parses the URL, validates host/port/scheme, builds the
right tunnel pointing at the URL's host:port, and dispatches to
tools_mcp_http::execute.

3 unit tests cover URL parsing failure modes (invalid URL, missing
host, unsupported scheme).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: CLI subcommand `tools-mcp http <METHOD> <URL>`

**Files:**
- Modify: `src/cli/args.rs` (add `Commands::Http`)
- Modify: `src/cli/handler.rs` (handle the new variant; add `execute_http`)

- [ ] **Step 1: Add the `Http` variant**

In `src/cli/args.rs`, append a new variant to the `Commands` enum AFTER the existing `Mysql` and `Redis` variants:

```rust
    /// Execute an HTTP request
    #[command(override_usage = "tools-mcp [GLOBAL OPTIONS] http [OPTIONS] <METHOD> <URL>")]
    #[command(after_help = USAGE_LEGEND)]
    Http {
        /// HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS).
        method: String,

        /// Full URL (http:// or https://).
        url: String,

        /// Extra header `Name: Value`. Repeat for multiple headers.
        #[arg(long = "header", short = 'H', help_heading = "HTTP")]
        headers: Vec<String>,

        /// Request body (raw string).
        #[arg(long, help_heading = "HTTP", conflicts_with = "data_file")]
        data: Option<String>,

        /// Read request body from a file path.
        #[arg(long = "data-file", help_heading = "HTTP", conflicts_with = "data")]
        data_file: Option<std::path::PathBuf>,

        /// Set Content-Type: application/json (does not transform the body).
        #[arg(long, help_heading = "HTTP")]
        json: bool,

        /// `Authorization: Bearer <TOKEN>` shortcut.
        #[arg(long, help_heading = "HTTP", conflicts_with = "basic")]
        bearer: Option<String>,

        /// HTTP Basic auth as `user:password`.
        #[arg(long, help_heading = "HTTP", conflicts_with = "bearer")]
        basic: Option<String>,

        /// Accept invalid TLS certificates (e.g. self-signed). DANGER: only
        /// use for trusted internal services.
        #[arg(long, help_heading = "HTTP")]
        insecure: bool,

        /// Print full ExecutionResult table (status + headers + body) instead
        /// of just the body. Default: print body only.
        #[arg(long = "include-headers", short = 'i', help_heading = "HTTP")]
        include_headers: bool,
    },
```

- [ ] **Step 2: Wire the handler**

In `src/cli/handler.rs`, add a new arm to the `match cli.command.clone()` block in `handle()`. Place it after `Redis` and before `None`:

```rust
    Some(Commands::Http {
        method,
        url,
        headers,
        data,
        data_file,
        json,
        bearer,
        basic,
        insecure,
        include_headers,
    }) => {
        Self::execute_http(
            &cli,
            method,
            url,
            headers,
            data,
            data_file,
            json,
            bearer,
            basic,
            insecure,
            include_headers,
        )
        .await
    }
```

Then append a new `execute_http` method to `impl CliHandler`:

```rust
    async fn execute_http(
        cli: &Cli,
        method: String,
        url: String,
        headers: Vec<String>,
        data: Option<String>,
        data_file: Option<std::path::PathBuf>,
        json: bool,
        bearer: Option<String>,
        basic: Option<String>,
        insecure: bool,
        include_headers: bool,
    ) -> Result<()> {
        // Parse `Name: Value` strings into pairs.
        let mut header_pairs: Vec<(String, String)> = Vec::new();
        for raw in headers {
            let (name, value) = raw.split_once(':').ok_or_else(|| {
                Error::Config(format!(
                    "--header '{raw}' must be 'Name: Value' (missing ':')"
                ))
            })?;
            header_pairs.push((name.trim().to_string(), value.trim().to_string()));
        }
        if json {
            header_pairs.push((
                "Content-Type".to_string(),
                "application/json".to_string(),
            ));
        }

        // Body
        let body: Option<Vec<u8>> = match (data, data_file) {
            (Some(s), None) => Some(s.into_bytes()),
            (None, Some(path)) => {
                let bytes = std::fs::read(&path).map_err(|e| {
                    Error::Config(format!(
                        "cannot read --data-file '{}': {e}",
                        path.display()
                    ))
                })?;
                Some(bytes)
            }
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
        };

        // Auth
        let auth = match (bearer, basic) {
            (Some(token), None) => tools_mcp_http::HttpAuth::Bearer(token),
            (None, Some(creds)) => {
                let (user, password) = creds.split_once(':').ok_or_else(|| {
                    Error::Config("--basic must be 'user:password'".to_string())
                })?;
                tools_mcp_http::HttpAuth::Basic {
                    user: user.to_string(),
                    password: password.to_string(),
                }
            }
            (None, None) => tools_mcp_http::HttpAuth::None,
            (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
        };

        let req = tools_mcp_http::HttpRequestSpec {
            method,
            url,
            headers: header_pairs,
            body,
            auth,
            insecure,
        };

        let tunnel_config = Self::cli_to_tunnel_config(cli)?;
        let result = crate::core::http::execute(req, tunnel_config).await?;

        if include_headers {
            println!("{}", CliFormatter::format(&result));
        } else {
            // Default: print just the body row (the last row, by construction).
            if let Some(body_row) = result.rows.last() {
                if body_row.len() >= 2 && body_row[0] == "body" {
                    println!("{}", body_row[1]);
                } else {
                    // Fallback if row layout drifts: print the whole table.
                    println!("{}", CliFormatter::format(&result));
                }
            }
        }
        Ok(())
    }
```

- [ ] **Step 3: Verify**

Run: `cargo build`
Expected: clean.

Run: `cargo run -q -- http --help 2>&1 | head -25`
Expected output includes:

```
Execute an HTTP request

Usage: tools-mcp [GLOBAL OPTIONS] http [OPTIONS] <METHOD> <URL>

Arguments:
  <METHOD>  HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS)
  <URL>     Full URL (http:// or https://)

Options:
      --config <CONFIG>  Path to YAML config file
  -h, --help             Print help

HTTP:
  -H, --header <HEADERS>     Extra header `Name: Value`. Repeat for multiple headers
      --data <DATA>          Request body (raw string)
      --data-file <DATA_FILE>  Read request body from a file path
      --json                 Set Content-Type: application/json ...
      --bearer <BEARER>      `Authorization: Bearer <TOKEN>` shortcut
      --basic <BASIC>        HTTP Basic auth as `user:password`
      --insecure             Accept invalid TLS certificates ...
  -i, --include-headers      Print full ExecutionResult table ...

Tunnel:
      --tunnel <TUNNEL>      ...
      ...
```

Run: `cargo test`
Expected: prior count passes (no new tests in this task).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(cli): add 'http <METHOD> <URL>' subcommand

Curl-style: positional method + URL; -H/--header (repeatable),
--data / --data-file (mutually exclusive), --json (sets Content-Type),
--bearer / --basic (mutually exclusive), --insecure (TLS), -i/
--include-headers (default: print body only).

Reuses the global --tunnel/--ssh-* flags via cli_to_tunnel_config and
delegates to core::http::execute. Phase 6 doesn't apply Profile/YAML
merging for HTTP — only CLI flags + global tunnel.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: MCP `http_exec` tool

**Files:**
- Modify: `src/mcp/tools.rs` (add `HttpExecParams` + helpers + entry function)
- Modify: `src/mcp/server.rs` (register the tool)
- Modify: `tests/mcp_smoke.rs` (assert all three tools list)

- [ ] **Step 1: Add `HttpExecParams` + helpers in `src/mcp/tools.rs`**

Append to `src/mcp/tools.rs`:

```rust
/// JSON parameters for the `http_exec` MCP tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HttpExecParams {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS).
    pub method: String,

    /// Full URL (http:// or https://).
    pub url: String,

    /// Extra headers as a list of `Name: Value` strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<String>,

    /// Request body (raw string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,

    /// Set Content-Type: application/json (does not transform the body).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub json: bool,

    /// `Authorization: Bearer <TOKEN>` shortcut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer: Option<String>,

    /// HTTP Basic auth as `user:password`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basic: Option<String>,

    /// Accept invalid TLS certificates (self-signed). DANGER.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure: bool,

    /// Tunnel kind. "direct" (default) or "ssh".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,

    /// SSH jump host(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,

    /// SSH jump user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,

    /// SSH jump password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,

    /// SSH key path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,

    /// SSH jump port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
}

fn http_params_to_request_and_tunnel(
    p: HttpExecParams,
) -> Result<(tools_mcp_http::HttpRequestSpec, Option<TunnelConfig>)> {
    let mut header_pairs: Vec<(String, String)> = Vec::new();
    for raw in &p.headers {
        let (name, value) = raw.split_once(':').ok_or_else(|| {
            Error::Config(format!("header '{raw}' must be 'Name: Value' (missing ':')"))
        })?;
        header_pairs.push((name.trim().to_string(), value.trim().to_string()));
    }
    if p.json {
        header_pairs.push((
            "Content-Type".to_string(),
            "application/json".to_string(),
        ));
    }

    let auth = match (p.bearer, p.basic) {
        (Some(token), None) => tools_mcp_http::HttpAuth::Bearer(token),
        (None, Some(creds)) => {
            let (user, password) = creds.split_once(':').ok_or_else(|| {
                Error::Config("basic must be 'user:password'".to_string())
            })?;
            tools_mcp_http::HttpAuth::Basic {
                user: user.to_string(),
                password: password.to_string(),
            }
        }
        (None, None) => tools_mcp_http::HttpAuth::None,
        (Some(_), Some(_)) => {
            return Err(Error::Config(
                "bearer and basic are mutually exclusive".to_string(),
            ));
        }
    };

    let req = tools_mcp_http::HttpRequestSpec {
        method: p.method,
        url: p.url,
        headers: header_pairs,
        body: p.data.map(|s| s.into_bytes()),
        auth,
        insecure: p.insecure,
    };

    let tunnel_config = build_tunnel_config_for_http(
        p.tunnel,
        p.ssh_jump,
        p.ssh_user,
        p.ssh_password,
        p.ssh_key_path,
        p.ssh_port,
    )?;

    Ok((req, tunnel_config))
}

fn build_tunnel_config_for_http(
    kind: Option<TunnelKind>,
    ssh_jump: Option<SshJumpInput>,
    ssh_user: Option<String>,
    ssh_password: Option<String>,
    ssh_key_path: Option<String>,
    ssh_port: Option<u16>,
) -> Result<Option<TunnelConfig>> {
    let Some(kind) = kind else { return Ok(None); };
    match kind {
        TunnelKind::Direct => {
            let stray = ssh_jump.is_some()
                || ssh_user.is_some()
                || ssh_password.is_some()
                || ssh_key_path.is_some()
                || ssh_port.is_some();
            if stray {
                return Err(Error::Config(
                    "ssh_* fields are only valid with tunnel = \"ssh\"".to_string(),
                ));
            }
            Ok(Some(TunnelConfig::Direct))
        }
        TunnelKind::Ssh => {
            let jumps = ssh_jump.map(SshJumpInput::into_jumps).ok_or_else(|| {
                Error::Config("ssh_jump is required when tunnel = \"ssh\"".to_string())
            })?;
            if jumps.is_empty() {
                return Err(Error::Config("ssh_jump must not be empty".to_string()));
            }
            let ssh_user = ssh_user.ok_or_else(|| {
                Error::Config("ssh_user is required when tunnel = \"ssh\"".to_string())
            })?;
            Ok(Some(TunnelConfig::Ssh {
                ssh_jumps: jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port: ssh_port.unwrap_or(22),
            }))
        }
    }
}

/// Public entry point for the http_exec tool.
pub async fn http_exec(params: HttpExecParams) -> Result<ExecutionResult> {
    let (req, tunnel_config) = http_params_to_request_and_tunnel(params)?;
    crate::core::http::execute(req, tunnel_config).await
}
```

(`build_tunnel_config_for_http` is yet another sibling of the existing MySQL/Redis tunnel-config builders. Same Phase 5 reasoning: a shared helper is justified once a third service exists, but extracting it without breaking the existing two builders adds change risk for this Phase. Defer the extraction to a Phase 7 cleanup task if desired.)

Add a unit test inside the existing `mod tests {}` block:

```rust
    #[test]
    fn test_http_params_to_request_basic() {
        let p = HttpExecParams {
            method: "POST".into(),
            url: "https://api.example.com/x".into(),
            headers: vec!["X-Foo: bar".into()],
            data: Some(r#"{"a":1}"#.into()),
            json: true,
            bearer: Some("tok".into()),
            basic: None,
            insecure: false,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let (req, tunnel) = http_params_to_request_and_tunnel(p).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "https://api.example.com/x");
        assert!(req.headers.contains(&("X-Foo".to_string(), "bar".to_string())));
        assert!(req.headers.contains(&("Content-Type".to_string(), "application/json".to_string())));
        assert_eq!(req.body.as_deref(), Some(r#"{"a":1}"#.as_bytes()));
        match req.auth {
            tools_mcp_http::HttpAuth::Bearer(t) => assert_eq!(t, "tok"),
            other => panic!("expected Bearer, got {other:?}"),
        }
        assert!(tunnel.is_none());
    }
```

(`HttpAuth` derives `Debug` already from Task 2, so `{other:?}` works.)

- [ ] **Step 2: Register the tool in `src/mcp/server.rs`**

Append to the existing `impl ToolsMcpServer` block (next to `mysql_exec` and `redis_exec`):

```rust
    /// Execute an HTTP request, optionally through an SSH tunnel.
    #[tool(description = "Send an HTTP/HTTPS request and return status, headers, and body. Optionally route through an SSH jump host. Same options as the `tools-mcp http` CLI subcommand.")]
    async fn http_exec(
        &self,
        Parameters(params): Parameters<crate::mcp::tools::HttpExecParams>,
    ) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        match crate::mcp::tools::http_exec(params).await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("serialize result failed: {e}"), None)
                })?;
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json),
                ]))
            }
            Err(e) => Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::Content::text(e.to_string()),
            ])),
        }
    }
```

- [ ] **Step 3: Update `tests/mcp_smoke.rs` for the third tool**

Find the existing block that tracks `found_mysql` + `found_redis`. Add `found_http`:

```rust
    let mut found_mysql = false;
    let mut found_redis = false;
    let mut found_http = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        if n == 0 {
            break;
        }
        if line.contains("\"id\":2") {
            if line.contains("mysql_exec") {
                found_mysql = true;
            }
            if line.contains("redis_exec") {
                found_redis = true;
            }
            if line.contains("http_exec") {
                found_http = true;
            }
            break;
        }
    }
```

And update the assertion block:

```rust
    assert!(found_mysql, "tools/list missing mysql_exec");
    assert!(found_redis, "tools/list missing redis_exec");
    assert!(found_http, "tools/list missing http_exec");
```

- [ ] **Step 4: Verify**

Run: `cargo test`
Expected: prior + 1 new (`test_http_params_to_request_basic`); mcp_smoke now asserts all 3 tools.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(mcp): http_exec tool registered on the rmcp server

ToolsMcpServer now exposes mysql_exec / redis_exec / http_exec. All
three: JSON params -> build typed request + tunnel config -> call
the matching core::<svc>::execute -> return ExecutionResult JSON.

mcp_smoke integration test asserts all three tools show up in
tools/list.

build_tunnel_config_for_http is yet another duplicate of the
mysql/redis tunnel builders — deferring the shared-helper extraction
to a Phase 7 cleanup.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Plugin assets — `/http` slash command + `http-using` skill

**Files:**
- Create: `commands/http.md`
- Create: `skills/http-using/SKILL.md`

- [ ] **Step 1: `/http` slash command**

Create `commands/http.md`:

```markdown
---
name: http
description: Send an HTTP request through the tools-mcp `http_exec` MCP tool, optionally via an SSH tunnel.
argument-hint: <METHOD> <URL> [-- ARGS]
---

# /http

Send this HTTP request via the `http_exec` MCP tool from the tools-mcp plugin:

```
$ARGUMENTS
```

## How to call it

1. **Parse the user's input.** First two tokens after `/http` are method + URL.
   Common shapes:
   - `/http GET https://api.example.com/users`
   - `/http POST https://api.example.com/users --data '{"name":"alice"}' --json`
   - `/http GET https://internal.api/health --tunnel ssh --ssh-jump bastion --ssh-user admin`

2. **Translate flags into MCP tool params:**
   - `-H "Name: Value"` → append to `headers` array (one element per `-H`).
   - `--data 'body'` → `data` field.
   - `--json` → `json: true` (sets Content-Type).
   - `--bearer TOKEN` / `--basic user:pass` → `bearer` / `basic` field (mutually exclusive).
   - `--insecure` → `insecure: true` (only for trusted internal services).
   - `--tunnel ssh --ssh-jump h1[,h2,...] --ssh-user u` → set `tunnel`/`ssh_jump`/`ssh_user`/etc.

3. **Call `http_exec`** with the params from Step 2.

4. **Render the result.** The response is an ExecutionResult with rows like
   `["status_code", "200"]`, `["status", "200 OK"]`, `["header.<name>", "<value>"]`,
   and finally `["body", "<...>"]`. By default show only the body unless the
   user asked for headers (e.g. `-i` / `--include-headers`).

5. **Destructive methods** (`POST` / `PUT` / `DELETE` / `PATCH`) on production
   URLs: pause and confirm with the user BEFORE calling the tool, especially
   when no `--data` was given (the user may have meant `GET`).

## When something fails

- `Error::Config("invalid URL ...")` → fix the URL (must include `http://` or `https://`).
- `Error::Service("HTTP: ...")` → reqwest error: connection refused, TLS handshake failure, DNS, etc.
- TLS cert errors on internal services → `--insecure` if the user OK's it; otherwise install the right CA cert on the host.
- SSH tunnel errors → use the **ssh-bastion-checklist** skill.
```

- [ ] **Step 2: `http-using` skill**

Create `skills/http-using/SKILL.md`:

```markdown
---
name: http-using
description: Use when calling the `http_exec` MCP tool from the tools-mcp plugin — explains parameter shape, output mapping (status / header.* / body rows), tunnel routing for internal HTTPS services, and common error shapes.
---

# Using the `http_exec` MCP tool

`tools-mcp` exposes an `http_exec` MCP tool. Sends one HTTP/HTTPS request and returns status + headers + body in a flat ExecutionResult. Phase 6: no profile/YAML support — just CLI/MCP fields.

## Tool input

```json
{
  "method":  "POST",                       // required
  "url":     "https://api.example.com/x",  // required
  "headers": ["X-Trace: abc", "X-Key: ..."],
  "data":    "{\"foo\":1}",
  "json":    true,                         // adds Content-Type: application/json
  "bearer":  "...token...",                // OR
  "basic":   "user:password",
  "insecure": false,                       // self-signed cert? set to true (rare)
  "tunnel":  "ssh",                        // optional
  "ssh_jump": "bastion.com",
  "ssh_user": "admin"
}
```

`method` is uppercased automatically (lowercase input is fine). Supported: GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS.

## Tunnel routing for internal HTTPS

When `tunnel = "ssh"`, `tools-mcp` opens an SSH chain to the bastion(s) and binds a local TCP listener (e.g. `127.0.0.1:50123`). reqwest's DNS is overridden so the URL's host (e.g. `api.internal.com`) resolves to that local listener — but **TLS SNI, Host header, and cert verification all use the original hostname**. So HTTPS through SSH tunnels works without any special TLS config; the cert just has to be valid for the URL's hostname.

If the cert is self-signed and you trust the target: `insecure: true`. Don't do this on the public internet.

## Output shape

ExecutionResult:

| field | value |
| --- | --- |
| `status_code` | `200` |
| `status` | `200 OK` |
| `header.content-type` | `application/json; charset=utf-8` |
| `header.content-length` | `142` |
| ... one row per header ... |
| `body` | `{"users":[...]}` |

Body is UTF-8-decoded if possible; binary bodies render as `<N bytes (non-UTF-8 body)>`.

When formatting the result for the user:
- Default: print just the body (look up the row with field == `"body"`).
- If the user asked for headers (or for debugging): print the whole table.
- If the body is JSON: pretty-print it before showing.

## Common error shapes

- `Error::Config("invalid URL '...': ...")` → URL didn't parse.
- `Error::Config("URL '...' has no host")` → e.g. `file:///` URLs.
- `Error::Config("URL '...' uses an unsupported scheme '...' (need http/https)")` → e.g. `ftp://`.
- `Error::Service("HTTP: error sending request ...")` → reqwest networking error: DNS, connect refused, TLS, etc.
- `Error::Service("HTTP body: ...")` → response body read failed mid-stream.
- `Error::Connection("tunnel endpoint ... is not a SocketAddr ...")` → only happens if the tunnel returns a hostname instead of an IP. Bug in the tunnel impl, not a user error.
- SSH tunnel errors → see the **ssh-bastion-checklist** skill.

## Read vs write

GET / HEAD / OPTIONS: safe to fire.
POST / PUT / DELETE / PATCH: confirm with the user BEFORE calling the tool. Especially watch for missing `data` — the user may have meant GET but typed POST.

## What this skill is NOT

- Not for streaming downloads / WebSocket / SSE (Phase 6+ might add).
- Not for HTML scraping per se — but you can fetch pages and the body comes back as a string, you can grep / extract from there.
- Not for `mysql_exec` or `redis_exec` — see the respective skills.
```

- [ ] **Step 3: Verify the files exist**

Run: `ls commands/ skills/`
Expected: `commands/http.md`, `commands/mysql.md`, `commands/redis.md`, plus the existing skill directories AND the new `skills/http-using/SKILL.md`.

- [ ] **Step 4: Commit**

```bash
git add commands/http.md skills/http-using/
git commit -m "feat(plugin): add /http slash command + http-using skill

- /http <METHOD> <URL> [flags] — calls http_exec with curl-style flags.
- http-using skill — input shape, tunnel routing for internal HTTPS,
  output mapping (status_code / status / header.* / body rows),
  common error shapes, read-vs-write confirmation guidance.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Documentation + final verification

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `AGENTS.md`

- [ ] **Step 1: README — Status section**

Replace the existing `## Status` section. Update the "implemented" list and remove HTTP-related items from "not yet implemented" if any:

```markdown
## Status

This is the Phase 6 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`) and `mysql_exec` MCP tool.
- Redis CLI mode (`tools-mcp redis "..."`) and `redis_exec` MCP tool.
- **HTTP CLI mode** (`tools-mcp http GET https://...`) and `http_exec` MCP tool.
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
  for MySQL and Redis. (HTTP profile/YAML is Phase 7+.)
- Direct connection (`--tunnel=direct` or no `--tunnel`).
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth. Host keys accepted with a fingerprint warning.
  Works for HTTP too — internal HTTPS services accessible via bastion.
- MCP server mode (`tools-mcp` with no subcommand) over stdio.

Not yet implemented:
- SSH direct connection (`tools-mcp ssh ...`)
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
- HTTP profile/YAML config (base_url, default headers, default bearer)
- HTTP/SSE MCP transport (the SERVER's transport, not the http tool)
- Redis cluster routing, pub/sub, transactions, scripting (EVAL)
- Per-Value typed mapping for RESP3 `Map` / `Set` / `Push`
```

- [ ] **Step 2: README — Usage subsection for HTTP**

After the existing `### Redis` subsection (and before `### MCP Server`), insert:

````markdown
### HTTP

```bash
# Simple GET
tools-mcp http GET https://api.example.com/users

# POST with JSON body
tools-mcp http POST https://api.example.com/users \
  --json --data '{"name":"alice"}' \
  --bearer "$API_TOKEN"

# Through an SSH jump to an internal HTTPS service
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  http GET https://internal-api.local/health

# Self-signed cert internal service
tools-mcp http GET https://10.0.0.5/api --insecure -i
```
````

- [ ] **Step 3: README — Plugin assets list**

Update the "What the plugin provides" block:

```markdown
What the plugin provides:

- **MCP tools** auto-registered via `.mcp.json`:
  - `mysql_exec` — run a MySQL query.
  - `redis_exec` — run a Redis command.
  - `http_exec` — send an HTTP request.
- **Skills** that guide the assistant:
  - `tools-mcp-using` — parameter shape, three-layer config priority, multi-hop syntax (mysql + redis).
  - `mysql-debugging` — diagnostic queries for common MySQL errors, locks, slow queries.
  - `redis-using` — Redis command shape, output mapping, destructive-command list.
  - `http-using` — HTTP tool input, tunnel routing for internal HTTPS, output mapping.
  - `ssh-bastion-checklist` — narrows down SSH-tunnel failures.
- **Slash commands**:
  - `/mysql <SQL>` — quick MySQL query.
  - `/redis <COMMAND>` — quick Redis command.
  - `/http <METHOD> <URL>` — quick HTTP request.
```

- [ ] **Step 4: CLAUDE.md and AGENTS.md updates**

Apply identical edits to both files.

a) **Project Overview lead sentence** — update to Phase 6:

Before:
```markdown
`tools-mcp` is a Rust CLI + MCP server for SSH, MySQL, and Redis. **Phase 5 (current) implements MySQL + Redis CLI modes and matching MCP tools (`mysql_exec`, `redis_exec`)**; SSH direct is the remaining service phase boundary.
```

After:
```markdown
`tools-mcp` is a Rust CLI + MCP server for HTTP, MySQL, Redis, and SSH. **Phase 6 (current) implements MySQL + Redis + HTTP CLI modes and matching MCP tools (`mysql_exec`, `redis_exec`, `http_exec`)**; SSH direct is the remaining service phase boundary.
```

b) **Module map** — add a new row for `tools-mcp-http` after `tools-mcp-redis`, and a row for the orchestrator after `core::redis`:

```markdown
| `tools-mcp-http` (lib) | `HttpRequestSpec` (method/url/headers/body/auth/insecure), `HttpExecutor::run(client, req)` (reqwest send + Response → ExecutionResult), and the entry `execute(tunnel, url_host, url_port, req) -> ExecutionResult`. Owns the `reqwest 0.12` (rustls-tls + gzip + brotli) dep. Maps responses to flat `field`/`value` rows (status_code / status / header.* / body). |
```

```markdown
| `tools-mcp` bin (root `src/core/http.rs`) | Orchestrator `execute(HttpRequestSpec, Option<TunnelConfig>)`: parse URL, derive host:port, build the right tunnel pointing at it, call into the http lib. Doesn't take a `Config` because Phase 6 deferred Profile/YAML for HTTP. |
```

c) **Phase boundaries** — add a new entry for HTTP after Redis:

```markdown
- **HTTP subcommand**: implemented in Phase 6. `tools-mcp http <METHOD> <URL>` and the `http_exec` MCP tool both route through `core::http::execute`. Tunnel routing uses reqwest's `resolve(host, addr)` override so HTTPS through SSH tunnels preserves SNI / Host header / cert verification. Phase 6 deliberately doesn't support Profile/YAML for HTTP — only CLI flags + global tunnel.
```

d) **Conventions worth knowing** — append:

```markdown
- **Service-specific Profile/YAML support is opt-in**: MySQL and Redis use the 3-layer merge (TOML profile → YAML → CLI args); HTTP currently doesn't (Phase 6 simplification). When adding a new service, decide upfront whether profile support is in scope; if yes, add the relevant fields to `Profile` and `Config` and a `build_config_<svc>` sibling in `cli/handler.rs`. If no, follow the HTTP pattern — orchestrator takes a typed request struct + `Option<TunnelConfig>` directly, no `Config` plumbing.
```

- [ ] **Step 5: Verify CLAUDE.md and AGENTS.md only differ on the cross-link**

Run: `diff <(tail -n +5 CLAUDE.md) <(tail -n +5 AGENTS.md)`
Expected: only the cross-link blockquote line + pre-existing methodology trailer line difference.

- [ ] **Step 6: Final workspace verification**

Run: `cargo test`
Expected: all tests pass. Report the actual count.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean. If diff shows up, run `cargo fmt --all` and re-verify.

Run: `cargo build --release`
Expected: workspace builds; binary at `target/release/tools-mcp`.

Run: `./target/release/tools-mcp http --help | head -3`
Expected:
```
Execute an HTTP request

Usage: tools-mcp [GLOBAL OPTIONS] http [OPTIONS] <METHOD> <URL>
```

Run regression checks for the existing subcommands:
- `./target/release/tools-mcp mysql --help | head -3`
- `./target/release/tools-mcp redis --help | head -3`
Both should still print their respective `Execute a MySQL query` / `Execute a Redis command` headers.

- [ ] **Step 7: Commit**

```bash
git add README.md CLAUDE.md AGENTS.md
git commit -m "docs: document Phase 6 HTTP support

- README Status: Phase 6 with mysql_exec + redis_exec + http_exec;
  HTTP usage examples; updated plugin asset list.
- CLAUDE.md / AGENTS.md: lead sentence updated; module map adds
  tools-mcp-http lib + core::http orchestrator rows; Phase
  boundaries record HTTP as shipped (only SSH-direct remains);
  conventions add a 'service-specific Profile/YAML support is
  opt-in' note explaining when to use the build_config_<svc>
  pattern vs the typed-request-struct pattern.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Summary

After Phase 6:

- `tools-mcp http <METHOD> <URL>` works as a CLI subcommand with curl-style flags.
- The `http_exec` MCP tool exposes the same surface to AI clients.
- Both share the orchestrator at `core::http::execute`.
- The new `tools-mcp-http` lib crate owns the `reqwest` dep; nothing else changes.
- The plugin ships a `/http` slash command + an `http-using` skill.
- HTTPS through SSH tunnels works correctly: SNI / Host header / cert verification all use the URL's host, not the tunnel's local listener.
- Architecture remains: every CLI subcommand has a paired MCP tool. **For services without Profile/YAML support (Phase 6 HTTP)**, the orchestrator takes a typed request struct directly instead of a Config.

**Deferred to Phase 7+:**
- HTTP profile/YAML config (`base_url`, default headers, default bearer).
- SSH-direct subcommand + tool.
- Redis cluster routing, pub/sub, transactions.
- Per-Value typed mapping for RESP3 `Map` / `Set` / `Push`.
- HTTP cookie jar, redirects beyond reqwest default, streaming downloads, WebSocket / SSE.
- Cleanup pass: extract a shared `build_tunnel_config_for_<svc>` helper across mysql/redis/http MCP tools.
