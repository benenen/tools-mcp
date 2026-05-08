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
