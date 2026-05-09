---
name: tools-mcp-using
description: Use when calling any of the four tools-mcp MCP tools (`mysql_exec` / `redis_exec` / `http_exec` / `ssh_exec`). Covers parameter shape per service, three-layer config priority (mysql/redis), SSH tunnel syntax (single + multi-hop), output mapping, destructive-command list, and common error shapes.
---

# Using the tools-mcp MCP tools

`tools-mcp` exposes four MCP tools — one per service. All four return a `{columns, rows, affected_rows}` ExecutionResult and accept tunnel fields. MySQL and Redis additionally accept `profile` / `config` for 3-layer config merge; HTTP and SSH-direct take their fields directly.

| Tool | Required input | Tunnel | Profile/YAML |
| --- | --- | --- | --- |
| `mysql_exec` | `query` + `host` + `user` | yes | yes |
| `redis_exec` | `command` + `host` | yes | yes |
| `http_exec` | `method` + `url` | yes | no |
| `ssh_exec` | `command` + `host` + `user` + (`password` OR `key_path`) | yes | no |

## Tool input shapes

```json
// mysql_exec
{ "query": "SELECT 1", "profile": "prod", "database": "myapp" }
{ "query": "SELECT 1", "host": "db", "user": "alice", "password": "..." }

// redis_exec — `command` parsed via shlex; quoted args supported
{ "command": "GET foo", "host": "redis", "password": "...", "db": 0 }
{ "command": "EVAL \"return 1\" 0", "profile": "prod-cache" }

// http_exec — no profile/YAML
{ "method": "GET", "url": "https://api.example.com/x" }
{ "method": "POST", "url": "...", "data": "{...}", "json": true, "bearer": "..." }
{ "method": "GET", "url": "...", "headers": ["X-Trace: abc"], "insecure": true }

// ssh_exec — TARGET creds (user/password/key_path) separate from JUMP creds (ssh_*)
{ "command": "uptime", "host": "server", "user": "admin", "key_path": "/home/me/.ssh/id_rsa" }
```

All four tools also accept the same tunnel fields:
```
"tunnel": "ssh",                  // "direct" (default) | "ssh"
"ssh_jump": "bastion.com",        // string OR comma-separated OR JSON array
"ssh_user": "jumper",
"ssh_password": "...",            // OR ssh_key_path
"ssh_port": 22
```

`tunnel="direct"` rejects stray `ssh_*` fields with a clear error.

## Three-layer config priority (mysql / redis only)

Low → high: TOML profile (`~/.config/tools-mcp/config.toml [profiles.<NAME>]`) → YAML file (`config: /path.yaml`) → explicit fields. Each layer fills `Option<...>` fields; later layers overwrite. Use a profile to avoid pasting credentials repeatedly; override per call only what differs (e.g. `database`, `query`).

HTTP and SSH-direct have no profile/YAML — pass all fields explicitly.

## SSH tunnel syntax

```json
// Single hop
{"tunnel": "ssh", "ssh_jump": "bastion.com", "ssh_user": "admin", "ssh_password": "..."}

// Multi-hop (Client → Bastion1 → Bastion2 → Target)
{"tunnel": "ssh", "ssh_jump": "b1.com,b2.com", "ssh_user": "admin", "ssh_key_path": "..."}
{"tunnel": "ssh", "ssh_jump": ["b1.com", "b2.com"], ...}
```

All hops share the same `ssh_user` / `ssh_password` / `ssh_key_path` / `ssh_port`. Per-hop overrides not supported yet.

For **`ssh_exec`** specifically: the TARGET creds (`user`, `password` / `key_path`, `port`) and the JUMP creds (`ssh_*`) are independent. The tool never infers one from the other — supply both even when they happen to be the same.

For **`http_exec`** through SSH: TLS SNI / Host header / cert verification all use the URL's original hostname; the tunnel only redirects DNS to a local listener. HTTPS-via-tunnel works without TLS surgery.

## Output mapping

**mysql_exec** — standard `{columns, rows, affected_rows}`. DML returns empty rows + non-zero affected_rows.

**redis_exec** — single `result` column; rows depend on the Redis Value:

| Redis Value | rows |
| --- | --- |
| Nil | empty |
| Int / BulkString / SimpleString / Okay | 1 row |
| Array | one row per element (HGETALL flattens to alternating field/value rows) |
| Map / Set / Push / RESP3-only | 1 row, Debug-formatted (known limitation) |

**http_exec** — flat `field`/`value` rows: `status_code`, `status`, `header.<name>` (one per response header), `body`. Body is UTF-8 if possible, else `<N bytes (non-UTF-8 body)>`. When showing to the user: default to printing just the `body` row; print the whole table only if the user asked for headers or for debugging.

**ssh_exec** — three rows: `exit_code` (`0` = success; `<unknown>` if channel closed without exit status, treat as failure), `stdout`, `stderr`.

## Destructive commands — confirm with the user FIRST

- **mysql_exec**: any `DROP`, `TRUNCATE`, `DELETE`, `UPDATE` without a `WHERE`, `ALTER`, `GRANT`, `REVOKE`. Treat as a privileged shell.
- **redis_exec**: `FLUSHDB`, `FLUSHALL`, `DEL` / `UNLINK` against more than a single named key, `DEBUG FLUSHALL`, `CONFIG SET`, `CLUSTER FORGET` / `MEET`, `RENAME` / `RENAMENX` (silently overwrites). `KEYS *` on prod can block — prefer `SCAN`.
- **http_exec**: `POST` / `PUT` / `DELETE` / `PATCH`. Watch for missing `data` — user may have typed POST when they meant GET.
- **ssh_exec**: `rm` / `find ... -delete`, `mv` overwrite, `dd`, `mkfs.*`, `systemctl restart` / `reboot` / `shutdown`, `apt install` / `apt remove`, `kill -9` / `pkill`, anything starting with `sudo`.

Read-only operations (`SELECT`, `GET` / `EXISTS` / `INFO`, `GET` / `HEAD`, `ls` / `cat` / `df` / `ps` / `systemctl status` / `journalctl`) are safe to run without a confirmation prompt.

## Common error shapes

- `Error::Config("MySQL host is required")` / `("Redis host is required")` / `("SSH target requires --password or --key-path")` — final merged config missing a required field. Profile wrong, YAML wrong, or fill it in explicitly.
- `Error::Config("invalid URL ...")` / `("URL ... uses an unsupported scheme ...")` — http_exec URL parse failure or non-http/https scheme (ftp:// / file://).
- `Error::Service("MySQL: ...")` / `("Redis: NOAUTH" / "WRONGTYPE" / "MOVED ...")` / `("HTTP: ...")` / `("SSH session open failed")` — service-side error. Read the message for the cause.
- `Error::Connection("SSH connect ... failed")` / `("SSH publickey/password auth failed")` — wrong creds or unreachable host. For `ssh_exec`: jump creds vs target creds are separate.
- `Error::Execution("failed to parse Redis command (unbalanced quotes?)")` — shlex parsing failed.
- Any `SSH tunnel ... failed` / multi-hop drop → escalate to `ssh-bastion-checklist`.
- MySQL-specific (1045 / 1146 / 1062 / deadlock / slow query / processlist) → escalate to `mysql-debugging`.

## PTY / TTY limitation (ssh_exec)

`ssh_exec` does NOT allocate a PTY. Commands needing a TTY (`top`, `htop`, `vim`, `passwd`, anything calling `isatty(stdin)`) will fail or behave unexpectedly. Use non-interactive variants (`top -bn1`, etc.) or wrap in `bash -c '...'`.

## What this skill is NOT

- Not a tutorial on each service — assume the user knows the SQL / Redis / shell command they want.
- Not for streaming / WebSocket / SSE / SCP/SFTP / Redis cluster routing / pub-sub / scripting orchestration (future phases).
- Not a debugging skill — see `mysql-debugging` for MySQL diagnostics, `ssh-bastion-checklist` for SSH tunnel troubleshooting.
