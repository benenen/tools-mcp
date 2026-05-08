# tools-mcp

Unified tool for SSH, MySQL, and Redis connections with MCP (Model Context Protocol) support.

## Features

- **CLI Mode**: Execute commands directly from the command line
- **MCP Mode**: Run as an MCP server for AI assistant integration (coming soon)
- **Configuration**: Support for TOML profiles and YAML config files
- **SSH Jump Host**: Access internal services through bastion hosts (coming soon)

## Status

This is the Phase 3 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`)
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
- Direct connection (`--tunnel=direct` or no `--tunnel` flag)
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth (`--ssh-password` / `--ssh-key-path`).
  Host keys are accepted with a fingerprint warning (a future phase will add strict checking).
- **MCP server mode** (run `tools-mcp` with no subcommand): exposes a `mysql_exec`
  tool to AI clients over stdio. Same connection / tunnel / profile options as the CLI.

Not yet implemented:
- Redis support
- SSH direct connection (`tools-mcp ssh ...`)
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification
- HTTP/SSE MCP transport

## Installation

Build a release binary and install it on `PATH`:

```bash
cargo install --path crates/tools-mcp
# or, for an unpublished build:
cargo build --release && cp target/release/tools-mcp ~/.local/bin/
```

`cargo install --path crates/tools-mcp` puts the binary at
`~/.cargo/bin/tools-mcp`, which is on `PATH` by default after a normal
Rust toolchain install.

This repo is a Cargo workspace with three crates under `crates/`:
`tools-mcp` (the binary), `tools-mcp-core` (the trait floor), and
`tools-mcp-mysql` (the MySQL primitives). `cargo build` / `cargo test`
from the workspace root build/test all three.

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

Prerequisite: `cargo install --path crates/tools-mcp` so the `tools-mcp` binary is on `PATH`.

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

- **MCP tool** `mysql_exec` (auto-registered via `.mcp.json`).
- **Skills** that guide the assistant when working with this tool:
  - `tools-mcp-using` — parameter shape, three-layer config priority, multi-hop syntax.
  - `mysql-debugging` — diagnostic queries for common MySQL error codes, locks, slow queries.
  - `ssh-bastion-checklist` — narrows down SSH-tunnel failures (TCP / auth / inside-tunnel / hang).
- **Slash command** `/mysql <SQL>` — quick query through the MCP tool, using the project's recorded profile.

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
