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
- `Error::Config("URL '...' uses an unsupported scheme '...' (need http/https)")` → e.g. `ftp://` or `file://`.
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
