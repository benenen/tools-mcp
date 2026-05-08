---
name: mysql
description: Run a MySQL query through the tools-mcp `mysql_exec` MCP tool, using the project's default profile if one is recorded.
argument-hint: <SQL query>
---

# /mysql

Run this SQL query against MySQL via the `mysql_exec` MCP tool from the tools-mcp plugin:

```
$ARGUMENTS
```

## How to call it

1. **Pick a connection.** In order of preference:
   - If the user's CLAUDE.md / AGENTS.md / memory records a default tools-mcp profile or YAML config for this project, pass it as `profile` or `config` in the tool call. Do NOT paste the password back into the call when a profile already covers it.
   - Otherwise, ask the user once for host/user/password (and database, tunnel, ssh_*) and remember it for the rest of the session.

2. **Call the tool.** Invoke `mysql_exec` with `query=$ARGUMENTS` plus whatever connection params Step 1 resolved. If the user clearly wrote the query against a specific database (e.g. `SELECT … FROM resource.users`), don't override `database`.

3. **Render the result.** If `rows` is non-empty, format as a Markdown table with the `columns` as headers. If empty, show `affected_rows`. Keep rows under 50 unless the user asked for more — paginate if needed.

4. **Destructive statements** (`UPDATE`/`DELETE`/`DROP`/`TRUNCATE` without a `WHERE` clause that the user clearly intended): pause and confirm with the user BEFORE calling the tool. The MCP tool runs anything you give it.

## When something fails

- Tool errors about missing host/user, profile not found → use the **tools-mcp-using** skill.
- MySQL error codes (1045/1146/1062/1213/2003/…) → use the **mysql-debugging** skill.
- SSH tunnel errors → use the **ssh-bastion-checklist** skill.
