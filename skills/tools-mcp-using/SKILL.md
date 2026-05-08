---
name: tools-mcp-using
description: Use when calling the `mysql_exec` MCP tool from the tools-mcp plugin — explains parameter shape, three-layer config priority, SSH tunnel syntax (single + multi-hop), and how to choose between profile / YAML / explicit fields.
---

# Using the `mysql_exec` MCP tool

`tools-mcp` exposes one MCP tool: `mysql_exec`. It runs a single MySQL query and returns the result as a structured JSON object. Same connection options as the CLI.

## Tool input shape

```json
{
  "query": "SELECT 1",                  // required

  // pick one of these connection sources, or layer them (see priority)
  "profile":  "prod",                   // ~/.config/tools-mcp/config.toml [profiles.prod]
  "config":   "/path/to/mysql.yaml",    // YAML config file
  // ...or fill in fields directly:
  "host":     "db.example.com",
  "port":     3306,
  "user":     "alice",
  "password": "...",
  "database": "myapp",

  // tunnel (optional; default = direct)
  "tunnel":       "ssh",                // "direct" | "ssh"
  "ssh_jump":     "bastion.com",        // string OR comma-separated OR JSON array
  "ssh_user":     "admin",
  "ssh_password": "...",
  "ssh_key_path": "/home/user/.ssh/id_rsa",
  "ssh_port":     22
}
```

## Three-layer config priority (low → high)

1. **TOML profile** — `~/.config/tools-mcp/config.toml` `[profiles.<NAME>]` when `profile` is set
2. **YAML file** — when `config` is set
3. **Explicit fields** in the tool call (highest)

Each layer fills in `Option<...>` fields; later layers overwrite earlier. Use this to avoid pasting credentials repeatedly: keep them in a profile, then override only `database` or `query` per call.

## Choosing the connection source

- **Have a working profile?** Just pass `{"query": "...", "profile": "prod"}`. Don't repeat host/user/password — that's noise and a leak risk.
- **Need a one-off override** (e.g. different database)? `{"query": "...", "profile": "prod", "database": "staging"}`.
- **Ad-hoc connection**? Pass all fields explicitly. Don't invent `profile` names that don't exist.
- **Running with --tunnel=ssh from CLI today**? Same shape: include `tunnel="ssh"` and the `ssh_*` fields.

## SSH tunnel syntax

Single hop:
```json
{"tunnel": "ssh", "ssh_jump": "bastion.com", "ssh_user": "admin", "ssh_password": "..."}
```

Multi-hop (Client → Bastion1 → Bastion2 → Target):
```json
{"tunnel": "ssh", "ssh_jump": "b1.com,b2.com", "ssh_user": "admin", "ssh_key_path": "..."}
```
or as JSON array:
```json
{"tunnel": "ssh", "ssh_jump": ["b1.com", "b2.com"], "ssh_user": "admin", "ssh_key_path": "..."}
```

All hops share the same `ssh_user`/`ssh_password`/`ssh_key_path`/`ssh_port`. Per-hop overrides aren't supported yet.

`tunnel="direct"` rejects any stray `ssh_*` fields with a clear error — don't pass them unless you're tunneling.

## Result shape

```json
{
  "columns": ["id", "name"],
  "rows": [["1", "Alice"], ["2", "Bob"]],
  "affected_rows": 2
}
```

For DML (INSERT/UPDATE/DELETE), `rows` is empty and `affected_rows` reflects the change count.

## When the tool errors

- `MySQL host is required` / `MySQL user is required` — final merged config is missing the field. Either the profile is wrong, the YAML is wrong, or you need to fill in the field explicitly.
- `SSH tunnel ... failed` — escalate to the `ssh-bastion-checklist` skill.
- `SQL syntax error` / `Table doesn't exist` — escalate to the `mysql-debugging` skill.

## Read vs. write

`mysql_exec` runs ANY SQL — `DROP`, `TRUNCATE`, `DELETE`, etc. all execute. **Before running a destructive statement, confirm with the user.** Treat the tool as you would a `mysql` shell open as a privileged user.

## What this skill is NOT

- Not a debugging skill — see `mysql-debugging`.
- Not an SSH troubleshooting skill — see `ssh-bastion-checklist`.
- Not for redis/ssh-direct/MCP-resource calls — those don't exist yet.
