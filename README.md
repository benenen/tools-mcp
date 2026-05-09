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

### MySQL

```bash
# Direct connection
tools4a mysql "SELECT * FROM users" --host=localhost --user=root --password=secret

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
# Direct connection
tools4a pgsql "SELECT * FROM users LIMIT 5" --host=localhost --user=postgres --password=secret --database=myapp

# Through an SSH jump
tools4a --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-key-path=~/.ssh/id_rsa \
  pgsql --host=pg.internal --user=app --password=app_pwd --database=myapp "SELECT NOW()"

# Using a TOML profile
tools4a pgsql "SELECT count(*) FROM events" --profile=prod-postgres
```

### MongoDB

Mongo commands are JSON documents passed to `Database::run_command`:

```bash
# find
tools4a mongo '{"find":"users","filter":{"active":true},"limit":5}' \
  --host=localhost --database=myapp

# insert
tools4a mongo '{"insert":"events","documents":[{"type":"signup","ts":1}]}' \
  --host=mongo.internal --user=app --password=secret --database=analytics

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

### Use as a Claude Code plugin

This repo ships a Claude Code plugin (`.claude-plugin/plugin.json` +
`.mcp.json` + `skills/`). Loading the plugin gives Claude the six
service MCP tools plus the project-specific skills — all wired up
automatically.

Prerequisite: `cargo install --path .` so the `tools4a` binary is on `PATH`.

Then in Claude Code:

```bash
/plugin marketplace add /path/to/tools4a        # one-time
/plugin install tools4a                          # enable the plugin
```

Or, for ad-hoc loading without going through a marketplace:

```bash
claude --plugin-dir /path/to/tools4a
```

What the plugin provides:

- **MCP tools** auto-registered via `.mcp.json`:
  - `mysql_exec` — run a MySQL query.
  - `pgsql_exec` — run a PostgreSQL query.
  - `redis_exec` — run a Redis command.
  - `mongo_exec` — run a MongoDB command (JSON document to `runCommand`).
  - `http_exec` — send an HTTP request.
  - `ssh_exec` — run a shell command on a remote SSH server.
- **Skills** that guide the assistant:
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
