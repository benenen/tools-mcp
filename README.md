# tools4a

Unified tool for SSH, MySQL, and Redis connections with MCP (Model Context Protocol) support.

## Features

- **CLI Mode**: Execute commands directly from the command line
- **MCP Mode**: Run as an MCP server for AI assistant integration (coming soon)
- **Configuration**: Support for TOML profiles and YAML config files
- **SSH Jump Host**: Access internal services through bastion hosts (coming soon)

## Status

This is the Phase 9 release. Currently implemented:

- All six service orchestrators (`MysqlOrchestrator`, `PgsqlOrchestrator`, `RedisOrchestrator`, `MongoOrchestrator`, `HttpOrchestrator`, `SshDirectOrchestrator`) impl the `tools4a_core::Service` trait, defined as `async fn execute(Self::Request, Option<TunnelConfig>) -> Result<ExecutionResult>`. They live in the `tools4a-orchestrator` lib crate.
- MySQL CLI mode (`tools4a mysql "..."`) and `mysql_exec` MCP tool.
- **PostgreSQL CLI mode** (`tools4a pgsql "..."`) and `pgsql_exec` MCP tool.
- Redis CLI mode (`tools4a redis "..."`) and `redis_exec` MCP tool.
- **MongoDB CLI mode** (`tools4a mongo '{"find":"coll","filter":{}}'`) and `mongo_exec` MCP tool — JSON document passed to `Database::run_command`.
- HTTP CLI mode (`tools4a http GET https://...`) and `http_exec` MCP tool.
- **SSH-direct CLI mode** (`tools4a ssh "..."`) and `ssh_exec` MCP tool —
  run a shell command on a target SSH server, optionally through SSH jump hosts.
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
  for MySQL, PostgreSQL, Redis, and MongoDB. (HTTP and SSH-direct profile/YAML not yet supported.)
- Direct connection (`--tunnel=direct` or no `--tunnel`).
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth. Host keys accepted with a fingerprint warning.
  Works for all six services.
- MCP server mode (`tools4a` with no subcommand) over stdio.

Not yet implemented:
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
- SSH PTY allocation (interactive commands like `top` won't work)
- HTTP / SSH-direct profile/YAML config
- HTTP/SSE MCP transport (the SERVER's transport)
- Redis cluster routing, pub/sub, transactions, scripting (EVAL)
- Per-Value typed mapping for RESP3 `Map` / `Set` / `Push`
- SCP/SFTP file transfer

## Installation

Build a release binary and install it on `PATH`:

```bash
cargo install --path .
# or, for an unpublished build:
cargo build --release && cp target/release/tools4a ~/.local/bin/
```

`cargo install --path .` puts the binary at
`~/.cargo/bin/tools4a`, which is on `PATH` by default after a normal
Rust toolchain install.

This repo is a Cargo workspace. The `tools4a` binary crate lives at
the repo root (presentation layer only). The lib crates under `crates/`
are: `tools4a-core` (trait floor + `Service` trait + `TunnelConfig`),
`tools4a-mysql` / `tools4a-pgsql` / `tools4a-redis` / `tools4a-mongo` /
`tools4a-http` / `tools4a-ssh` (per-service primitives), and
`tools4a-orchestrator` (Config/Profile/Loader/Merger + Tunnel impls +
the six `<Svc>Orchestrator: impl Service`).
`cargo build` / `cargo test` from the root build and test all of them.

## Usage

### Read-only by default (mysql, pgsql, mongo)

The `mysql`, `pgsql`, and `mongo` subcommands (and their MCP equivalents)
**reject write operations by default**. Use `--allow-write`
(`allow_write: true` in MCP) to opt in.

| Service | Reads (always allowed)                    | Writes (need `--allow-write`)                                 |
| ------- | ----------------------------------------- | ------------------------------------------------------------- |
| mysql   | `SELECT`, `SHOW`, `EXPLAIN`, `DESCRIBE`, `WITH`, `VALUES`, `TABLE` | `INSERT`, `UPDATE`, `DELETE`, `REPLACE`, `CREATE`, `DROP`, `ALTER`, `TRUNCATE`, `GRANT`, `CALL`, `SET`, … |
| pgsql   | same as mysql                             | same as mysql, plus `COPY`, `VACUUM`, `ANALYZE`, etc.         |
| mongo   | `find`, `aggregate` (no `$out`/`$merge`), `count`, `distinct`, `listCollections`, `listDatabases`, `listIndexes`, `dbStats`, `collStats`, `serverStatus`, `ping`, `hello`, `buildInfo`, `getParameter`, … | `insert`, `update`, `delete`, `findAndModify`, `drop`, `create`, `createIndexes`, `aggregate` with `$out` / `$merge`, … |

For mysql + pgsql, when `--allow-write` is **not** set, the session is
also forced into a DB-level read-only mode (`SET SESSION TRANSACTION
READ ONLY` / `SET default_transaction_read_only = on`) as a second line
of defense — so a misclassified write will still be rejected by the
database itself. Mongo has no per-session read-only mode, so the
orchestrator-level command whitelist is the only guard.

Redis, HTTP, and SSH are **not** restricted — they accept any command
without `--allow-write`.

### MySQL

```bash
# Direct connection (read-only by default)
tools4a mysql "SELECT * FROM users" --host=localhost --user=root --password=secret

# Write — requires --allow-write
tools4a mysql "INSERT INTO users (name) VALUES ('alice')" \
  --host=localhost --user=root --password=secret --allow-write

# Using YAML config
tools4a --config=mysql.yaml mysql "SELECT * FROM users"

# Using TOML profile
tools4a mysql "SELECT * FROM users" --profile=prod

# Through a single SSH jump
tools4a --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  mysql --host=mysql.internal --user=root --password=dbpass "SELECT 1"

# Through two SSH jumps (comma-separated; all share --ssh-user/--ssh-password)
tools4a --tunnel=ssh --ssh-jump=bastion1.com,bastion2.com --ssh-user=admin \
  --ssh-key-path=~/.ssh/jump_key \
  mysql --host=mysql.internal --user=root --password=dbpass "SELECT 1"
```

### PostgreSQL

```bash
# Direct connection (read-only)
tools4a pgsql "SELECT * FROM users LIMIT 5" --host=localhost --user=postgres --password=secret --database=myapp

# Write — requires --allow-write
tools4a pgsql "DELETE FROM events WHERE created_at < now() - interval '30 days'" \
  --host=localhost --user=postgres --password=secret --database=myapp --allow-write

# Through an SSH jump
tools4a --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-key-path=~/.ssh/id_rsa \
  pgsql --host=pg.internal --user=app --password=app_pwd --database=myapp "SELECT NOW()"

# Using a TOML profile
tools4a pgsql "SELECT count(*) FROM events" --profile=prod-postgres
```

### MongoDB

Mongo commands are JSON documents passed to `Database::run_command`:

```bash
# find (read — works without --allow-write)
tools4a mongo '{"find":"users","filter":{"active":true},"limit":5}' \
  --host=localhost --database=myapp

# insert (write — requires --allow-write)
tools4a mongo '{"insert":"events","documents":[{"type":"signup","ts":1}]}' \
  --host=mongo.internal --user=app --password=secret --database=analytics \
  --allow-write

# Through an SSH jump
tools4a --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=jpwd \
  mongo '{"listCollections":1}' --host=mongo.internal --database=admin
```

### Redis

```bash
# Direct connection
tools4a redis "GET mykey" --host=localhost --port=6379

# With password + db
tools4a redis "HGETALL myhash" --host=localhost --password=secret --db=2

# Through an SSH jump
tools4a --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  redis "INFO replication" --host=redis.internal --password=cache_pwd

# Using a TOML profile
tools4a redis "KEYS *" --profile=prod-cache
```

### HTTP

```bash
# Simple GET
tools4a http GET https://api.example.com/users

# POST with JSON body
tools4a http POST https://api.example.com/users \
  --json --data '{"name":"alice"}' \
  --bearer "$API_TOKEN"

# Through an SSH jump to an internal HTTPS service
tools4a --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  http GET https://internal-api.local/health

# Self-signed cert internal service (show full status + headers + body)
tools4a http GET https://10.0.0.5/api --insecure -i
```

### SSH (remote command execution)

```bash
# Direct connection
tools4a ssh "uname -a" --host=server.com --user=admin --key-path=~/.ssh/id_rsa

# With password
tools4a ssh "df -h" --host=10.0.0.5 --user=root --password=secret

# Through an SSH jump (jump creds are SEPARATE from target creds)
tools4a --tunnel=ssh --ssh-jump=bastion.com --ssh-user=jumper --ssh-password=jpwd \
  ssh "systemctl status nginx" --host=internal-server --user=admin --key-path=~/.ssh/target_key

# Show structured output (exit_code/stdout/stderr table)
tools4a ssh "false" --host=h --user=u --key-path=~/.ssh/k -i
```

By default `tools4a`'s exit code mirrors the remote command's exit code,
so shell-script usage works (e.g. `if tools4a ssh "test -f /etc/passwd" ...`).

### MCP Server

Run `tools4a` with no subcommand to start an MCP server over stdio:

```bash
tools4a
```

It exposes six tools (`mysql_exec`, `pgsql_exec`, `redis_exec`, `mongo_exec`,
`http_exec`, `ssh_exec`) — one per service. Each tool accepts the same
parameters as the corresponding CLI subcommand plus the shared tunnel fields
(`tunnel`, `ssh_jump`, `ssh_user`, `ssh_password`, `ssh_key_path`, `ssh_port`).
AI clients (Claude Desktop, Cursor, etc.) can call these tools to query
databases and run commands through SSH jump hosts.

Example MCP configuration entry (e.g. for Claude Desktop):

```json
{
  "mcpServers": {
    "tools4a": {
      "command": "/usr/local/bin/tools4a"
    }
  }
}
```

### Install in Claude Code

This repo ships a Claude Code plugin (`.claude-plugin/plugin.json` +
`.claude-plugin/marketplace.json` + `.mcp.json` + `skills/`). Once
installed, Claude gets the six service MCP tools plus the
project-specific skills — all wired up automatically.

Pick **one** of the two paths below.

#### Path A: Install as a plugin (recommended)

Gives you the MCP tools **and** the bundled skills (`tools4a-using`,
`mysql-debugging`, `ssh-bastion-checklist`).

```bash
# 1. Build & install the binary onto $PATH
cargo install --path .                              # produces ~/.cargo/bin/tools4a

# 2. In a Claude Code session, register this repo as a marketplace
/plugin marketplace add /absolute/path/to/tools4a   # one-time

# 3. Install the plugin from that marketplace
/plugin install tools4a@tools4a                     # enable plugin

# 4. Verify
/mcp                                                # should list `tools4a`
```

To upgrade after pulling new commits, rebuild the binary
(`cargo install --path . --force`) and re-run `/plugin marketplace update tools4a`.

#### Path B: Install as a plain MCP server (lighter)

Gives you the six MCP tools only (no skills). Useful if you don't want
plugin-level integration.

```bash
# 1. Build & install the binary
cargo install --path .

# 2. Register the MCP server with Claude Code
claude mcp add tools4a tools4a                      # name=tools4a, command=tools4a

# 3. Verify
claude mcp list                                     # should show `tools4a`
```

The `tools4a` binary speaks MCP over stdio when invoked with no
subcommand, so no extra flags are needed.

#### What the plugin provides

- **MCP tools** auto-registered via `.mcp.json`:
  - `mysql_exec` — run a MySQL query.
  - `pgsql_exec` — run a PostgreSQL query.
  - `redis_exec` — run a Redis command.
  - `mongo_exec` — run a MongoDB command (JSON document to `runCommand`).
  - `http_exec` — send an HTTP request.
  - `ssh_exec` — run a shell command on a remote SSH server.
- **Skills** that guide the assistant (Path A only):
  - `tools4a-using` — consolidated guide for all six tools: parameter shape per service, three-layer config priority (mysql + pgsql + redis + mongo), SSH tunnel syntax, output mapping, destructive-command list.
  - `mysql-debugging` — diagnostic queries for common MySQL errors, locks, slow queries.
  - `ssh-bastion-checklist` — narrows down SSH-tunnel failures.

### Configuration

**YAML Config** (`mysql.yaml`):
```yaml
type: mysql
host: localhost
port: 3306
user: root
password: secret
database: mydb
```

**TOML Config** (`~/.config/tools4a/config.toml`):
```toml
[profiles.prod]
type = "mysql"
host = "prod.example.com"
port = 3306
user = "app_user"
password = "secret"
```

## Development

Run tests:
```bash
cargo test
```

Build:
```bash
cargo build
```

## License

MIT
