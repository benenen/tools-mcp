---
name: browser-using
description: Use when calling the `browser_exec` MCP tool from tools4a — explains the agent-browser daemon model, session reuse, proxy passthrough, output mapping (exit_code/stdout/stderr), the Phase 1 SSH-tunnel deferral with its `ssh -D` workaround, and the install-it-yourself prerequisite.
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

## Tunneling to internal HTTPS (Phase 1 workaround)

Phase 1 does NOT support `tunnel = "ssh"` for the browser — the existing single-port `direct-tcpip` tunnel doesn't fit a full browser (cookies / SNI / Host header / sub-resources). If the user needs to reach an internal HTTPS service through a bastion:

1. They run `ssh -D 1080 <bastion>` themselves in a separate terminal, keeping it open.
2. Pass `"proxy": "socks5://127.0.0.1:1080"` to `browser_exec`.

Phase 2 will fold the SOCKS server into tools4a so `tunnel = "ssh"` works for browser directly. If a user asks for `tunnel = "ssh"` today, you'll get an `Error::Config` whose message itself contains the workaround (no skill lookup needed at the point of failure).

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
- Not for SOCKS tunneling through SSH (Phase 2). For now, instruct the user to set up `ssh -D` themselves and use `--proxy`.
- Not for `playwright` / `puppeteer` directly — those have their own MCP servers; this skill is specifically for the agent-browser surface.
