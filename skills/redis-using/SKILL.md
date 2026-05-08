---
name: redis-using
description: Use when calling the `redis_exec` MCP tool from the tools-mcp plugin — explains command-string syntax (shlex parsing), the `db` parameter, output mapping for common Redis types, and when to be careful with destructive commands.
---

# Using the `redis_exec` MCP tool

`tools-mcp` exposes a `redis_exec` MCP tool symmetric to `mysql_exec`. Same connection layer (profile / YAML / explicit fields, with optional SSH tunneling); different command shape.

## Tool input

```json
{
  "command":  "GET foo",                 // required, parsed via shlex
  "host":     "redis.internal",
  "port":     6379,
  "password": "...",
  "db":       0,
  "profile":  "prod-cache",
  "tunnel":   "ssh",
  "ssh_jump": "bastion.com",
  "ssh_user": "admin"
  // ...same ssh_* / config fields as mysql_exec
}
```

`command` is the Redis CLI command as a single string. shlex parsing handles quoted args:

- `SET key value`
- `SET key "a value with spaces"`
- `HSET h f1 v1 f2 v2`
- `LPUSH list a b c`
- `EVAL "return redis.call('GET', KEYS[1])" 1 mykey`

## Three-layer config priority (low → high)

1. **TOML profile** when `profile` is set
2. **YAML file** when `config` is set
3. **Explicit fields** in the tool call (highest)

Same as `mysql_exec`. See the `tools-mcp-using` skill for the merge mechanics.

## Output mapping

`redis_exec` returns an `ExecutionResult` (`columns` + `rows` + `affected_rows`) as JSON. Phase 5 maps the Redis response types simply:

| Redis Value | columns | rows | affected_rows |
|---|---|---|---|
| Nil | `["result"]` | `[]` | 0 |
| Int(N) | `["result"]` | `[[ "N" ]]` | 1 |
| BulkString("x") | `["result"]` | `[[ "x" ]]` | 1 |
| SimpleString("OK") / Okay | `["result"]` | `[[ "OK" ]]` | 1 |
| Array([a, b, c]) | `["result"]` | `[[ "a" ], [ "b" ], [ "c" ]]` | 3 |
| Map / Set / Push / VerbatimString / etc. (RESP3) | `["result"]` | `[[ "<Debug-formatted>" ]]` | 1 |

Practical examples:

- `LRANGE list 0 -1` → one row per list element.
- `HGETALL hash` → flat alternating field/value rows (Redis returns a flat array; the mapping reflects that).
- `INFO replication` → single bulk-string row with the entire INFO body.
- `EXISTS key` → integer row.
- `TYPE key` → status row (`string` / `list` / `hash` / `set` / `zset`).

If a user really needs structured Map/Set output (RESP3-only), the current Phase 5 mapping shows it Debug-formatted in a single row — that's a known limitation. A future phase may add proper key-value mapping for `Map`.

## Destructive commands

`redis_exec` runs anything you give it. Confirm with the user BEFORE running:

- `FLUSHDB` / `FLUSHALL`
- `DEL` / `UNLINK` against more than a single named key
- `DEBUG FLUSHALL`
- `CONFIG SET` (server-wide changes)
- `CLUSTER FORGET` / `CLUSTER MEET`
- `RENAME` / `RENAMENX` (silently overwrite the target)

Read-only commands (`GET`, `EXISTS`, `KEYS`, `SCAN`, `INFO`, `LRANGE`, `HGETALL`, `TYPE`, `TTL`, etc.) are safe to run without a confirmation prompt.

`KEYS *` on a production server can block the server for seconds — prefer `SCAN` for large datasets.

## Common error shapes

- `Error::Config("Redis host is required")` → connection params didn't merge to a usable host. Most likely: profile doesn't exist, or no host fields anywhere.
- `Error::Service("Redis: NOAUTH ...")` → password missing or wrong.
- `Error::Service("Redis: WRONGTYPE ...")` → command applied to the wrong key type (`HGET` against a string key, `LRANGE` against a hash, etc.).
- `Error::Service("Redis: MOVED <slot> <host>:<port>")` → the key lives on a different cluster node. tools-mcp doesn't follow cluster redirects (no cluster client). Connect to the target node directly via `host`/`port` overrides.
- `Error::Execution("failed to parse Redis command (unbalanced quotes?): ...")` → shlex couldn't parse the input. Check quote balance.
- `Error::Execution("empty Redis command")` → input was whitespace only.
- SSH errors → see the `ssh-bastion-checklist` skill.

## What this skill is NOT

- Not a Redis tutorial — assume the user knows the command they want.
- Not for cluster routing / pub-sub / transactions / scripting orchestration (Phase 6+).
- Not for `mysql_exec` — that's `tools-mcp-using`.
