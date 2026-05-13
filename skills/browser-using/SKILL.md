---
name: browser-using
description: Use when calling the `browser_exec` MCP tool from tools4a — explains the agent-browser daemon model, session reuse, proxy passthrough, output mapping (exit_code/stdout/stderr), SSH tunneling via the built-in SOCKS5 server, and the install-it-yourself prerequisite.
---

# Using the `browser_exec` MCP tool

`tools4a` exposes `browser_exec`, a thin wrapper around the externally-installed [`agent-browser`](https://github.com/vercel-labs/agent-browser) CLI. tools4a does not embed a browser; agent-browser's daemon owns all state (pages / cookies / storage / authentication). Each `browser_exec` call is one short-lived CLI invocation against that persistent daemon.

## Pre-requisite

The operator must have installed `agent-browser` separately. tools4a will not download it. If you get `Error::Config("agent-browser binary not found ...")`, stop and ask the user to install it (`npm i -g agent-browser` or the upstream Rust build). Do NOT try to install it on the user's behalf — they may want a specific version or install path.

## Tool input

```json
{
  "subcommand": "open",
  "args": ["https://example.com"],
  "session": "work",
  "proxy":   "socks5://127.0.0.1:1080",
  "proxy_bypass": "localhost,127.0.0.1",
  "browser_args": "--disable-gpu",
  "bin":     "/usr/local/bin/agent-browser"
}
```

`subcommand` and `args` are passed through verbatim — tools4a does not enumerate the agent-browser subcommand surface, so any new subcommand upstream works without a tools4a release. Common subcommands: `open`, `click`, `fill`, `type`, `snapshot`, `screenshot`, `eval`, `cookies`, `batch`, `get`, `is`, `back`, `forward`, `reload`, `wait`.

## Session model

agent-browser's daemon persists between calls. Pass the same `session` across calls to share state:

```
{ "subcommand": "open",  "args": ["https://gmail.com"], "session": "personal" }
{ "subcommand": "fill",  "args": ["#email", "me@example.com"], "session": "personal" }
{ "subcommand": "click", "args": ["#next"], "session": "personal" }
```

Omitting `session` uses agent-browser's default session.

## Output shape

ExecutionResult (3 rows, `field`/`value` columns):

| field | value |
| --- | --- |
| `exit_code` | `0` |
| `stdout` | `<command output, often JSON>` |
| `stderr` | `<diagnostic output if any>` |

On success: show `stdout` (parse as JSON if it starts with `{` or `[`). On failure (`exit_code != 0`): show `stderr` — it carries agent-browser's structured error message (page not loaded, selector not found, etc.).

## Tunneling via SSH (built-in SOCKS5)

Set `tunnel = "ssh"` plus the usual `ssh_jump` / `ssh_user` / etc. fields and tools4a will:

1. Build an SSH session chain to the bastion(s) — same `build_session_chain` helper the other six tools use.
2. Bind a SOCKS5 listener on `127.0.0.1:<random>`. Each accepted SOCKS5 CONNECT opens a fresh `direct-tcpip` channel through the SSH chain — the bastion does the actual TCP connect and DNS resolution.
3. Inject `--proxy socks5://127.0.0.1:<random>` into the `agent-browser` invocation. Chrome / agent-browser routes ALL of the page's traffic (HTTP, HTTPS, sub-resources, websockets) through that proxy, so internal HTTPS services with valid certs work without any tools4a-side TLS handling.
4. Tear the tunnel down on exit (close the listener, drop the SSH session).

**Conflict**: if `tunnel = "ssh"` AND `proxy = ...` are BOTH set, tools4a returns `Error::Config("conflict ...")` — pick one (drop `proxy` and let tools4a inject its own, or use `tunnel = "direct"` with your own proxy). Silently overriding would mask a likely user mistake.

**Manual workaround (still works)**: if you want to keep the SSH listener separately (e.g. multiple tools sharing one bastion), set `tunnel = "direct"` and `proxy = "socks5://127.0.0.1:1080"` after starting `ssh -D 1080 <bastion>` yourself. The inline `tunnel = "ssh"` form is preferred for browser-only use because tools4a owns the listener lifecycle.

## Destructive subcommands — confirm first

| Subcommand | Destructive? | Confirm if... |
| --- | --- | --- |
| `open`, `back`, `forward`, `reload`, `snapshot`, `screenshot`, `get *`, `is *`, `cookies` (read) | No | — |
| `click` | Maybe | the page is prod / the button looks irreversible (`Submit`, `Delete`, `Pay`) |
| `fill`, `type` | Yes (mutates page state) | prod forms, especially with PII |
| `eval` | Yes (arbitrary JS) | always |
| `network route` / `unroute` | Yes (rewrites traffic) | always |
| `cookies` (write) / `storage` (write) | Yes | always |

When in doubt: prefer `snapshot` first to confirm what's on the page, then act.

## What this skill is NOT

- Not for embedding a browser inside tools4a — agent-browser is external.
- Not for installing or upgrading agent-browser — tell the user to run their own install if missing.
- Not a proxy for arbitrary clients — the SOCKS5 listener tools4a binds for `tunnel = "ssh"` is **per-call** (torn down when `browser_exec` returns). If you need a persistent SOCKS proxy, use `ssh -D` directly.
- Not for `playwright` / `puppeteer` directly — those have their own MCP servers; this skill is specifically for the agent-browser surface.
