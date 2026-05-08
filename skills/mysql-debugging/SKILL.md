---
name: mysql-debugging
description: Use when a `mysql_exec` query fails with a MySQL error (1045/1146/1062/2003 etc.), runs slowly, or the user asks to diagnose locks, deadlocks, processlist, or query plans. Provides ready-made diagnostic queries to run via the same `mysql_exec` tool.
---

# Diagnosing MySQL via `mysql_exec`

Use this when a query returns a MySQL server error or behaves unexpectedly. Every diagnostic step below is a SQL statement you can hand back to `mysql_exec` with the same connection params.

## Common error codes

| Code | Meaning | First thing to check |
|---|---|---|
| `1045` | Access denied for user | Wrong user/password, or host not whitelisted in `mysql.user` |
| `1146` | Table `db.t` doesn't exist | Wrong `database` field, or wrong table name |
| `1062` | Duplicate entry for unique key | Application bug; show the row that conflicts: `SELECT * FROM t WHERE <uniq_col> = <value>` |
| `1213` | Deadlock found | Get the latest deadlock report (see Deadlocks below) |
| `1205` | Lock wait timeout exceeded | See `SHOW PROCESSLIST` + `information_schema.innodb_trx` |
| `2003` | Can't connect to MySQL server | Network / port / firewall — not a SQL issue. If `tunnel=ssh`, escalate to `ssh-bastion-checklist`. |
| `2013` | Lost connection during query | Likely a long query hit `wait_timeout`. `SHOW VARIABLES LIKE 'wait_timeout'` |

## Diagnostic queries

**Server status & version**
```sql
SELECT VERSION();
SHOW VARIABLES LIKE 'version_compile%';
SHOW STATUS LIKE 'Threads_connected';
```

**What's running right now**
```sql
SHOW FULL PROCESSLIST;
-- Or, more detail:
SELECT * FROM information_schema.processlist
 WHERE COMMAND <> 'Sleep' ORDER BY TIME DESC LIMIT 20;
```

**Active transactions / locks (InnoDB)**
```sql
SELECT * FROM information_schema.innodb_trx;
SELECT * FROM performance_schema.data_locks;       -- 8.0+
SELECT * FROM performance_schema.data_lock_waits;  -- 8.0+
```

**Most recent deadlock**
```sql
SHOW ENGINE INNODB STATUS;
-- Find the "LATEST DETECTED DEADLOCK" section in the output text.
```

**Query plan**
```sql
EXPLAIN <the slow query>;
EXPLAIN ANALYZE <the slow query>;   -- 8.0.18+, executes the query
```

**Indexes on a table**
```sql
SHOW INDEX FROM <db>.<table>;
SELECT * FROM information_schema.statistics
 WHERE table_schema = '<db>' AND table_name = '<table>';
```

**Schema introspection**
```sql
SHOW DATABASES;
SHOW TABLES IN <db>;
DESCRIBE <db>.<table>;        -- short form
SHOW CREATE TABLE <db>.<table>;
```

**Slow query log status**
```sql
SHOW VARIABLES LIKE 'slow_query%';
SHOW VARIABLES LIKE 'long_query_time';
```

**Replication state (if applicable)**
```sql
SHOW REPLICA STATUS\G        -- 8.0+ syntax
SHOW SLAVE STATUS\G          -- legacy
```

## Workflow

1. Read the error message verbatim. Identify the error code.
2. Pick the matching diagnostic query above; run it via `mysql_exec` with the SAME connection (profile / tunnel) the failing query used.
3. Summarize the diagnostic output to the user before recommending a fix.
4. If the user asks for a fix, propose it as a SQL change OR a config change — but **do not run destructive SQL without explicit confirmation** (see the `tools-mcp-using` skill).

## What this skill is NOT

- Not a tutorial on MySQL — assume the user knows SQL.
- Not connection / tunnel troubleshooting — that's `ssh-bastion-checklist` (when SSH is in the path) or basic network checks (when not).
