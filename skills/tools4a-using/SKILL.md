---
name: tools4a-using
description: Use when calling any of the eight tools4a MCP tools (`mysql_exec` / `pgsql_exec` / `clickhouse_exec` / `redis_exec` / `mongo_exec` / `http_exec` / `ssh_exec` / `browser_exec`). Covers parameter shape per service, three-layer config priority (mysql/pgsql/clickhouse/redis/mongo), SSH tunnel syntax (single + multi-hop), output mapping, destructive-command list, and common error shapes.
---

# Using the tools4a MCP tools

`tools4a` exposes eight MCP tools — one per service. All eight return a `{columns, rows, affected_rows}` ExecutionResult and accept tunnel fields. MySQL, PostgreSQL, ClickHouse, Redis, and MongoDB additionally accept `profile` / `config` for 3-layer config merge; HTTP, SSH-direct, and Browser take their fields directly. Browser also requires the external `agent-browser` binary to be installed separately on the host running tools4a.

| Tool | Required input | Tunnel | Profile/YAML |
| --- | --- | --- | --- |
| `mysql_exec` | `query` + `host` + `user` | yes | yes |
| `pgsql_exec` | `query` + `host` + `user` | yes | yes |
| `clickhouse_exec` | `query` + `host` | yes | yes |
| `redis_exec` | `command` + `host` | yes | yes |
| `mongo_exec` | `command` + `host` + `database` | yes | yes |
| `http_exec` | `method` + `url` | yes | no |
| `ssh_exec` | `command` + `host` + `user` + (`password` OR `key_path`) | yes | no |
| `browser_exec` | `subcommand` | yes (SOCKS5 over SSH) | no |

## Tool input shapes

```json
// mysql_exec
{ "query": "SELECT 1", "profile": "prod", "database": "myapp" }
{ "query": "SELECT 1", "host": "db", "user": "alice", "password": "..." }

// pgsql_exec — same shape as mysql_exec, default port 5432, no `db` field (use `database`)
{ "query": "SELECT 1", "profile": "prod-pg", "database": "myapp" }
{ "query": "SELECT 1", "host": "pg", "user": "app", "password": "...", "database": "myapp" }

// redis_exec — `command` parsed via shlex; quoted args supported
{ "command": "GET foo", "host": "redis", "password": "...", "db": 0 }
{ "command": "EVAL \"return 1\" 0", "profile": "prod-cache" }

// mongo_exec — `command` is a JSON OBJECT string (parsed → BSON → run_command)
{ "command": "{\"find\":\"users\",\"filter\":{\"x\":1}}", "profile": "prod-mongo" }
{ "command": "{\"insert\":\"events\",\"documents\":[{\"a\":1}]}", "host": "mongo", "database": "analytics" }

// http_exec — no profile/YAML
{ "method": "GET", "url": "https://api.example.com/x" }
{ "method": "POST", "url": "...", "data": "{...}", "json": true, "bearer": "..." }
{ "method": "GET", "url": "...", "headers": ["X-Trace: abc"], "insecure": true }

// ssh_exec — TARGET creds (user/password/key_path) separate from JUMP creds (ssh_*)
{ "command": "uptime", "host": "server", "user": "admin", "key_path": "/home/me/.ssh/id_rsa" }

// clickhouse_exec — SQL over HTTP, default port 8123, default user "default"
{ "query": "SELECT 1", "host": "ch", "user": "default" }
{ "query": "SELECT count() FROM events", "profile": "prod-ch" }

// browser_exec — shells out to agent-browser; sessions persist across calls
{ "subcommand": "open", "args": ["https://example.com"], "session": "work" }
{ "subcommand": "snapshot", "session": "work" }
// tunnel="ssh" works: tools4a binds a per-call SOCKS5 listener over the
// SSH chain and injects --proxy socks5://127.0.0.1:<rand> into agent-browser.
// If you set BOTH tunnel=ssh AND an explicit proxy, that's Error::Config (conflict).
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

## Three-layer config priority (mysql / pgsql / redis / mongo)

Low → high: TOML profile (`~/.config/tools4a/config.toml [profiles.<NAME>]`) → YAML file (`config: /path.yaml`) → explicit fields. Each layer fills `Option<...>` fields; later layers overwrite. Use a profile to avoid pasting credentials repeatedly; override per call only what differs (e.g. `database`, `query`).

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

**pgsql_exec** — standard `{columns, rows, affected_rows}` like mysql. Type mapping covers bool / int / float / text / date / time / timestamp / timestamptz; uncommon types (json, jsonb, uuid, arrays) render as `<typename>` placeholders.

**mongo_exec** — single `result` row containing the JSON-serialized result Document. For find-style commands the Document has shape `{"cursor": {"firstBatch": [...]}}`. For write commands it has `{"n": ..., "ok": 1}`. Caller parses the JSON string in the row to navigate the response.

**redis_exec** — single `result` column; rows depend on the Redis Value:

| Redis Value | rows |
| --- | --- |
| Nil | empty |
| Int / BulkString / SimpleString / Okay | 1 row |
| Array | one row per element (HGETALL flattens to alternating field/value rows) |
| Map / Set / Push / RESP3-only | 1 row, Debug-formatted (known limitation) |

**http_exec** — flat `field`/`value` rows: `status_code`, `status`, `header.<name>` (one per response header), `body`. Body is UTF-8 if possible, else `<N bytes (non-UTF-8 body)>`. When showing to the user: default to printing just the `body` row; print the whole table only if the user asked for headers or for debugging.

**ssh_exec** — three rows: `exit_code` (`0` = success; `<unknown>` if channel closed without exit status, treat as failure), `stdout`, `stderr`.

**clickhouse_exec** — standard `{columns, rows, affected_rows}` like mysql/pgsql. Result comes from ClickHouse's HTTP interface; DDL/DML returns empty rows + non-zero affected_rows.

**browser_exec** — three rows: `exit_code` (`0` = success), `stdout` (agent-browser's stdout, often JSON for structured subcommands like `snapshot`), `stderr` (diagnostic if any). Parse `stdout` as JSON when the subcommand documents JSON output; otherwise treat as plain text.

## Write gating (`allow_write`) — mysql_exec / pgsql_exec / mongo_exec

These three tools are **read-only by default**. Any write attempt is
rejected upfront with `Error::Service("write operation not allowed
without --allow-write (CLI) / allow_write=true (MCP)")`. Pass
`allow_write: true` in the MCP params to enable writes.

- **mysql_exec / pgsql_exec**: read-only first-keyword whitelist is
  `SELECT`, `SHOW`, `EXPLAIN`, `DESCRIBE` / `DESC`, `WITH`, `VALUES`,
  `TABLE`, `USE`. Anything else (INSERT/UPDATE/DELETE/DDL/etc.) needs
  `allow_write: true`. As a second line of defense, when
  `allow_write=false` the SQL session is forced into DB-level read-only
  (`SET SESSION TRANSACTION READ ONLY` for MySQL, `SET
  default_transaction_read_only = on` for Postgres).
- **mongo_exec**: read-only commands are `find`, `aggregate` (without
  `$out`/`$merge` stages), `count`, `distinct`, `listCollections`,
  `listDatabases`, `listIndexes`, `dbStats`, `collStats`,
  `serverStatus`, `ping`, `hello`, `buildInfo`, `getParameter`, etc.
  Writes (`insert`, `update`, `delete`, `findAndModify`, `drop`,
  `create`, `createIndexes`, aggregate-with-`$out`/`$merge`) need
  `allow_write: true`. Mongo has no per-session read-only mode, so the
  command whitelist is the only guard.
- **clickhouse_exec**: same SQL-keyword whitelist as mysql/pgsql plus
  ClickHouse-specific reads (`DESCRIBE TABLE`, `SHOW DATABASES`, etc.).
  When `allow_write=false` the HTTP call also sets `readonly=1` on the
  server side as a second line of defense.
- **redis_exec / http_exec / ssh_exec / browser_exec**: NOT gated. They
  accept any command/method without `allow_write` — Redis is
  shell-shaped, HTTP/SSH encode write semantics in their method/command,
  and browser actions are external-side-effect rather than tools4a-side.

## Destructive commands — confirm with the user FIRST

When `allow_write: true` is being passed (or for non-gated services),
still confirm before running anything destructive:

- **mysql_exec**: any `DROP`, `TRUNCATE`, `DELETE`, `UPDATE` without a `WHERE`, `ALTER`, `GRANT`, `REVOKE`. Treat as a privileged shell.
- **pgsql_exec**: `DROP`, `TRUNCATE`, `DELETE without WHERE`, `UPDATE without WHERE`, `GRANT`, `REVOKE`, `ALTER`. Same caution as mysql.
- **redis_exec**: `FLUSHDB`, `FLUSHALL`, `DEL` / `UNLINK` against more than a single named key, `DEBUG FLUSHALL`, `CONFIG SET`, `CLUSTER FORGET` / `MEET`, `RENAME` / `RENAMENX` (silently overwrites). `KEYS *` on prod can block — prefer `SCAN`.
- **mongo_exec**: `drop` (collection drop), `dropDatabase`, `delete` with broad filter, `update` with `"multi": true` + broad filter, `findAndModify` with `"remove": true`, admin commands `createUser` / `dropUser` / `grantRolesToUser`.
- **http_exec**: `POST` / `PUT` / `DELETE` / `PATCH`. Watch for missing `data` — user may have typed POST when they meant GET.
- **ssh_exec**: `rm` / `find ... -delete`, `mv` overwrite, `dd`, `mkfs.*`, `systemctl restart` / `reboot` / `shutdown`, `apt install` / `apt remove`, `kill -9` / `pkill`, anything starting with `sudo`.
- **clickhouse_exec**: `DROP`, `TRUNCATE`, `DELETE FROM`, `ALTER ... DROP`, `OPTIMIZE FINAL` (rewrites parts), `DETACH PARTITION`. ClickHouse-specific: avoid running `SELECT * FROM huge_table` without `LIMIT` on prod — the query streams the whole table over HTTP.
- **browser_exec**: `fill` / `type` on prod forms (PII), `click` on irreversible buttons (Submit, Delete, Pay), `eval` (arbitrary JS — always confirm), `network route` / `unroute` (rewrites traffic), `cookies` / `storage` writes. Prefer `snapshot` first to confirm page state before any state-changing subcommand. For per-service details see `browser-using`.

Read-only operations (`SELECT`, `GET` / `EXISTS` / `INFO`, `GET` / `HEAD`, `ls` / `cat` / `df` / `ps` / `systemctl status` / `journalctl`) are safe to run without a confirmation prompt.

## Common error shapes

- `Error::Config("MySQL host is required")` / `("Pgsql host is required")` / `("Pgsql user is required")` / `("Redis host is required")` / `("Mongo host is required")` / `("Mongo database is required")` / `("SSH target requires --password or --key-path")` — final merged config missing a required field. Profile wrong, YAML wrong, or fill it in explicitly.
- `Error::Config("invalid URL ...")` / `("URL ... uses an unsupported scheme ...")` — http_exec URL parse failure or non-http/https scheme (ftp:// / file://).
- `Error::Service("MySQL: ...")` / `("Pgsql: ...")` / `("Pgsql query: ...")` / `("Redis: NOAUTH" / "WRONGTYPE" / "MOVED ...")` / `("Mongo: ...")` / `("Mongo run_command: ...")` / `("HTTP: ...")` / `("SSH session open failed")` — service-side error. Read the message for the cause.
- `Error::Connection("SSH connect ... failed")` / `("SSH publickey/password auth failed")` — wrong creds or unreachable host. For `ssh_exec`: jump creds vs target creds are separate.
- `Error::Execution("failed to parse Redis command (unbalanced quotes?)")` — shlex parsing failed.
- `Error::Execution("failed to parse Mongo command as JSON: ...")` / `("failed to convert command JSON to BSON: ...")` / `("Mongo command must be a JSON object")` — mongo_exec command string is not valid JSON, not a JSON object, or cannot be converted to BSON.
- `Error::Config("agent-browser binary not found ...")` — operator must install agent-browser separately (`npm i -g agent-browser` or upstream Rust build). Don't auto-install.
- `Error::Config("tunnel=ssh and an explicit `proxy` field conflict ...")` — user set BOTH `tunnel=ssh` AND `proxy=...` on `browser_exec`. tools4a injects its own `--proxy socks5://...` from the SOCKS tunnel endpoint when ssh is set; pick one or the other.
- Any `SSH tunnel ... failed` / multi-hop drop → escalate to `ssh-bastion-checklist`.
- MySQL-specific (1045 / 1146 / 1062 / deadlock / slow query / processlist) → escalate to `mysql-debugging`.
- Browser-specific (agent-browser daemon issues, selector mismatches, page-load failures) → escalate to `browser-using`.

## PTY / TTY limitation (ssh_exec)

`ssh_exec` does NOT allocate a PTY. Commands needing a TTY (`top`, `htop`, `vim`, `passwd`, anything calling `isatty(stdin)`) will fail or behave unexpectedly. Use non-interactive variants (`top -bn1`, etc.) or wrap in `bash -c '...'`.

## What this skill is NOT

- Not a tutorial on each service — assume the user knows the SQL / Redis / shell command they want.
- Not for streaming / WebSocket / SSE / SCP/SFTP / Redis cluster routing / pub-sub / scripting orchestration (future phases).
- Not a debugging skill — see `mysql-debugging` for MySQL diagnostics, `ssh-bastion-checklist` for SSH tunnel troubleshooting.
