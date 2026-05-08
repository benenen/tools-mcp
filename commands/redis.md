---
name: redis
description: Run a Redis command through the tools-mcp `redis_exec` MCP tool, using the project's default profile if one is recorded.
argument-hint: <Redis command>
---

# /redis

Run this Redis command via the `redis_exec` MCP tool from the tools-mcp plugin:

```
$ARGUMENTS
```

## How to call it

1. **Pick a connection.** In order of preference:
   - If the user's CLAUDE.md / AGENTS.md / memory records a default tools-mcp Redis profile or YAML config for this project, pass it as `profile` or `config` in the tool call. Don't paste the password back into the call when a profile already covers it.
   - Otherwise, ask the user once for host/port/password/db (and tunnel/ssh_*) and remember it for the rest of the session.

2. **Call the tool.** Invoke `redis_exec` with `command=$ARGUMENTS` plus the connection params from Step 1. If the user's command refers to a specific db (e.g. `SELECT 5`), don't override `db`.

3. **Render the result.** If `rows` is non-empty, format as a Markdown table with the `columns` as headers. Empty rows = the command returned `nil`; show that explicitly.

4. **Destructive commands** (`FLUSHDB` / `FLUSHALL` / `DEL` / `UNLINK` against many keys, `CONFIG SET`, `DEBUG FLUSHALL`): pause and confirm with the user BEFORE calling the tool.

## When something fails

- Tool errors about missing host or profile not found → use the **tools-mcp-using** skill (the connection / profile / tunnel pipeline is shared with `mysql_exec`).
- SSH tunnel errors → use the **ssh-bastion-checklist** skill.
- Redis command errors (`WRONGTYPE`, `NOAUTH`, `MOVED` for cluster, `ERR unknown command`, etc.) → explain the cause to the user; suggest the right command shape if applicable.
