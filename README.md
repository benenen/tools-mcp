# tools-mcp

Unified tool for SSH, MySQL, and Redis connections with MCP (Model Context Protocol) support.

## Features

- **CLI Mode**: Execute commands directly from the command line
- **MCP Mode**: Run as an MCP server for AI assistant integration (coming soon)
- **Configuration**: Support for TOML profiles and YAML config files
- **SSH Jump Host**: Access internal services through bastion hosts (coming soon)

## Status

This is the Phase 2 release. Currently implemented:

- MySQL CLI mode (`tools-mcp mysql "..."`)
- Configuration via YAML file (`--config=PATH`) or TOML profile (`--profile=NAME`)
- Direct connection (`--tunnel=direct` or no `--tunnel` flag)
- SSH tunnel (`--tunnel=ssh`) with single- or multi-hop jump (`--ssh-jump=h1[,h2,...]`),
  password or key auth (`--ssh-password` / `--ssh-key-path`).
  Host keys are accepted with a fingerprint warning (Phase 3 will add strict checking).

Not yet implemented:
- Redis support
- SSH direct connection (`tools-mcp ssh ...`)
- MCP server mode (running without a subcommand prints a placeholder)
- SSH key passphrases, per-hop auth overrides, strict known_hosts verification

## Installation

```bash
cargo build --release
```

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
