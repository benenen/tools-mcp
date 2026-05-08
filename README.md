# tools-mcp

Unified tool for SSH, MySQL, and Redis connections with MCP (Model Context Protocol) support.

## Features

- **CLI Mode**: Execute commands directly from the command line
- **MCP Mode**: Run as an MCP server for AI assistant integration (coming soon)
- **Configuration**: Support for TOML profiles and YAML config files
- **SSH Jump Host**: Access internal services through bastion hosts (coming soon)

## Status

This is the Phase 6 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`) and `mysql_exec` MCP tool.
- Redis CLI mode (`tools-mcp redis "..."`) and `redis_exec` MCP tool.
- **HTTP CLI mode** (`tools-mcp http GET https://...`) and `http_exec` MCP tool.
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
  for MySQL and Redis. (HTTP profile/YAML is Phase 7+.)
- Direct connection (`--tunnel=direct` or no `--tunnel`).
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth. Host keys accepted with a fingerprint warning.
  Works for HTTP too — internal HTTPS services accessible via bastion.
- MCP server mode (`tools-mcp` with no subcommand) over stdio.

Not yet implemented:
- SSH direct connection (`tools-mcp ssh ...`)
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
- HTTP profile/YAML config (base_url, default headers, default bearer)
- HTTP/SSE MCP transport (the SERVER's transport, not the http tool)
- Redis cluster routing, pub/sub, transactions, scripting (EVAL)
- Per-Value typed mapping for RESP3 `Map` / `Set` / `Push`

## Installation

Build a release binary and install it on `PATH`:

```bash
cargo install --path .
# or, for an unpublished build:
cargo build --release && cp target/release/tools-mcp ~/.local/bin/
```

`cargo install --path .` puts the binary at
`~/.cargo/bin/tools-mcp`, which is on `PATH` by default after a normal
Rust toolchain install.

This repo is a Cargo workspace. The `tools-mcp` binary crate lives at
the repo root; the lib crates `tools-mcp-core` (the trait floor),
`tools-mcp-mysql` (MySQL primitives), and `tools-mcp-redis` (Redis
primitives) live under `crates/`. `cargo build` / `cargo test` from
the root build and test all of them.

## Usage

### MySQL

```bash
# Direct connection
tools-mcp mysql "SELECT * FROM users" --host=localhost --user=root --password=secret

# Using YAML config
tools-mcp --config=mysql.yaml mysql "SELECT * FROM users"

# Using TOML profile
tools-mcp mysql "SELECT * FROM users" --profile=prod

# Through a single SSH jump
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  mysql --host=mysql.internal --user=root --password=dbpass "SELECT 1"

# Through two SSH jumps (comma-separated; all share --ssh-user/--ssh-password)
tools-mcp --tunnel=ssh --ssh-jump=bastion1.com,bastion2.com --ssh-user=admin \
  --ssh-key-path=~/.ssh/jump_key \
  mysql --host=mysql.internal --user=root --password=dbpass "SELECT 1"
```

### Redis

```bash
# Direct connection
tools-mcp redis "GET mykey" --host=localhost --port=6379

# With password + db
tools-mcp redis "HGETALL myhash" --host=localhost --password=secret --db=2

# Through an SSH jump
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  redis "INFO replication" --host=redis.internal --password=cache_pwd

# Using a TOML profile
tools-mcp redis "KEYS *" --profile=prod-cache
```

### HTTP

```bash
# Simple GET
tools-mcp http GET https://api.example.com/users

# POST with JSON body
tools-mcp http POST https://api.example.com/users \
  --json --data '{"name":"alice"}' \
  --bearer "$API_TOKEN"

# Through an SSH jump to an internal HTTPS service
tools-mcp --tunnel=ssh --ssh-jump=bastion.com --ssh-user=admin --ssh-password=secret \
  http GET https://internal-api.local/health

# Self-signed cert internal service (show full status + headers + body)
tools-mcp http GET https://10.0.0.5/api --insecure -i
```

### MCP Server

Run `tools-mcp` with no subcommand to start an MCP server over stdio:

```bash
tools-mcp
```

It exposes one tool, `mysql_exec`, with the same parameters as the CLI's
`mysql` subcommand (host/port/user/password/database/profile + tunnel/ssh_*).
AI clients (Claude Desktop, Cursor, etc.) can call this tool to run MySQL
queries through SSH jump hosts.

Example MCP configuration entry (e.g. for Claude Desktop):

```json
{
  "mcpServers": {
    "tools-mcp": {
      "command": "/usr/local/bin/tools-mcp"
    }
  }
}
```

### Use as a Claude Code plugin

This repo ships a Claude Code plugin (`.claude-plugin/plugin.json` +
`.mcp.json` + `skills/` + `commands/`). Loading the plugin gives Claude
the `mysql_exec` MCP tool plus three project-specific skills and one
slash command — all wired up automatically.

Prerequisite: `cargo install --path .` so the `tools-mcp` binary is on `PATH`.

Then in Claude Code:

```bash
/plugin marketplace add /path/to/tools-mcp        # one-time
/plugin install tools-mcp                          # enable the plugin
```

Or, for ad-hoc loading without going through a marketplace:

```bash
claude --plugin-dir /path/to/tools-mcp
```

What the plugin provides:

- **MCP tools** auto-registered via `.mcp.json`:
  - `mysql_exec` — run a MySQL query.
  - `redis_exec` — run a Redis command.
  - `http_exec` — send an HTTP request.
- **Skills** that guide the assistant:
  - `tools-mcp-using` — parameter shape, three-layer config priority, multi-hop syntax (mysql + redis).
  - `mysql-debugging` — diagnostic queries for common MySQL errors, locks, slow queries.
  - `redis-using` — Redis command shape, output mapping, destructive-command list.
  - `http-using` — HTTP tool input, tunnel routing for internal HTTPS, output mapping.
  - `ssh-bastion-checklist` — narrows down SSH-tunnel failures.
- **Slash commands**:
  - `/mysql <SQL>` — quick MySQL query.
  - `/redis <COMMAND>` — quick Redis command.
  - `/http <METHOD> <URL>` — quick HTTP request.

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

**TOML Config** (`~/.config/tools-mcp/config.toml`):
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
