# Tools MCP Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the foundation for tools-mcp with MySQL support, configuration management (TOML + YAML), and CLI mode.

**Architecture:** Single Rust binary with modular design - config loading with priority merging, MySQL connection via mysql_async, CLI argument parsing with clap, and table-formatted output.

**Tech Stack:** Rust, clap, tokio, mysql_async, serde, toml, serde_yaml, anyhow, comfy-table

---

## Task 1: Project Setup and Dependencies

**Files:**
- Modify: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/error.rs`

- [ ] **Step 1: Update Cargo.toml with dependencies**

```toml
[package]
name = "tools-mcp"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "tools-mcp"
path = "src/main.rs"

[lib]
name = "tools_mcp"
path = "src/lib.rs"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
tokio = { version = "1.40", features = ["full"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
serde_yaml = "0.9"
mysql_async = "0.34"
comfy-table = "7.1"

[dev-dependencies]
tempfile = "3.12"
```

- [ ] **Step 2: Create lib.rs with module declarations**

```rust
pub mod config;
pub mod tunnel;
pub mod connection;
pub mod executor;
pub mod output;
pub mod cli;
pub mod error;

pub use error::{Error, Result};
```

- [ ] **Step 3: Create error.rs with error types**

```rust
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Config(String),
    Connection(String),
    Execution(String),
    Io(std::io::Error),
    Mysql(mysql_async::Error),
    Yaml(serde_yaml::Error),
    Toml(toml::de::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(msg) => write!(f, "Configuration error: {}", msg),
            Error::Connection(msg) => write!(f, "Connection error: {}", msg),
            Error::Execution(msg) => write!(f, "Execution error: {}", msg),
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Mysql(e) => write!(f, "MySQL error: {}", e),
            Error::Yaml(e) => write!(f, "YAML error: {}", e),
            Error::Toml(e) => write!(f, "TOML error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<mysql_async::Error> for Error {
    fn from(e: mysql_async::Error) -> Self {
        Error::Mysql(e)
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(e: serde_yaml::Error) -> Self {
        Error::Yaml(e)
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::Toml(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build`
Expected: Compilation succeeds with warnings about unused modules

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/lib.rs src/error.rs
git commit -m "feat: add project dependencies and error types

- Add clap, tokio, mysql_async, serde, yaml/toml support
- Define Error enum with conversions
- Set up library structure

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Configuration Types

**Files:**
- Create: `src/config/mod.rs`
- Create: `src/config/types.rs`

- [ ] **Step 1: Write test for ServiceType deserialization**

Create: `src/config/types.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_from_str() {
        assert_eq!("mysql".parse::<ServiceType>().unwrap(), ServiceType::Mysql);
        assert_eq!("redis".parse::<ServiceType>().unwrap(), ServiceType::Redis);
        assert_eq!("ssh".parse::<ServiceType>().unwrap(), ServiceType::Ssh);
        assert!("invalid".parse::<ServiceType>().is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_service_type_from_str`
Expected: FAIL with "ServiceType not defined"

- [ ] **Step 3: Implement ServiceType and configuration types**

Add to top of `src/config/types.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Mysql,
    Redis,
    Ssh,
}

impl FromStr for ServiceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mysql" => Ok(ServiceType::Mysql),
            "redis" => Ok(ServiceType::Redis),
            "ssh" => Ok(ServiceType::Ssh),
            _ => Err(format!("Invalid service type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelType {
    Direct,
    Ssh,
}

impl FromStr for TunnelType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "direct" => Ok(TunnelType::Direct),
            "ssh" => Ok(TunnelType::Ssh),
            _ => Err(format!("Invalid tunnel type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    #[serde(rename = "type")]
    pub service_type: ServiceType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub key_path: Option<String>,
    pub tunnel_type: Option<TunnelType>,
    pub ssh_jump: Option<String>,
    pub ssh_user: Option<String>,
    pub ssh_password: Option<String>,
    pub ssh_key_path: Option<String>,
    pub ssh_port: Option<u16>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TomlConfig {
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, Profile>,
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub service_type: Option<ServiceType>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub key_path: Option<String>,
    pub tunnel_type: Option<TunnelType>,
    pub ssh_jump: Option<String>,
    pub ssh_user: Option<String>,
    pub ssh_password: Option<String>,
    pub ssh_key_path: Option<String>,
    pub ssh_port: Option<u16>,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_service_type_from_str`
Expected: PASS

- [ ] **Step 5: Create config module entry point**

Create: `src/config/mod.rs`

```rust
mod types;
mod loader;
mod merger;

pub use types::{Config, Profile, ServiceType, TomlConfig};
pub use loader::ConfigLoader;
pub use merger::ConfigMerger;
```

- [ ] **Step 6: Commit**

```bash
git add src/config/
git commit -m "feat: add configuration types

- Define ServiceType enum (mysql, redis, ssh)
- Define Profile and Config structs
- Add FromStr implementation for ServiceType

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Configuration Loader

**Files:**
- Create: `src/config/loader.rs`
- Create: `tests/fixtures/test-config.toml`
- Create: `tests/fixtures/test-config.yaml`

- [ ] **Step 1: Write test for TOML config loading**

Create: `src/config/loader.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceType;

    #[test]
    fn test_load_toml_config() {
        let toml_content = r#"
[profiles.test]
type = "mysql"
host = "localhost"
port = 3306
user = "root"
"#;
        let config: TomlConfig = toml::from_str(toml_content).unwrap();
        let profile = config.profiles.get("test").unwrap();
        assert_eq!(profile.service_type, ServiceType::Mysql);
        assert_eq!(profile.host.as_deref(), Some("localhost"));
        assert_eq!(profile.port, Some(3306));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_load_toml_config`
Expected: FAIL with "TomlConfig not in scope"

- [ ] **Step 3: Implement ConfigLoader**

Add to top of `src/config/loader.rs`:

```rust
use crate::config::{Config, TomlConfig};
use crate::error::{Error, Result};
use std::path::Path;

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_toml_file(path: &Path) -> Result<TomlConfig> {
        let content = std::fs::read_to_string(path)?;
        let config: TomlConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_yaml_file(path: &Path) -> Result<Config> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_default_toml() -> Result<Option<TomlConfig>> {
        let home = std::env::var("HOME").map_err(|_| {
            Error::Config("HOME environment variable not set".to_string())
        })?;
        let config_path = Path::new(&home)
            .join(".config")
            .join("tools-mcp")
            .join("config.toml");

        if config_path.exists() {
            Ok(Some(Self::load_toml_file(&config_path)?))
        } else {
            Ok(None)
        }
    }
}
```

- [ ] **Step 4: Add Config deserialization support**

Add to `src/config/types.rs` after Config struct:

```rust
impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ConfigHelper {
            #[serde(rename = "type")]
            service_type: Option<ServiceType>,
            host: Option<String>,
            port: Option<u16>,
            user: Option<String>,
            password: Option<String>,
            database: Option<String>,
            key_path: Option<String>,
            tunnel_type: Option<TunnelType>,
            ssh_jump: Option<String>,
            ssh_user: Option<String>,
            ssh_password: Option<String>,
            ssh_key_path: Option<String>,
            ssh_port: Option<u16>,
        }

        let helper = ConfigHelper::deserialize(deserializer)?;
        Ok(Config {
            service_type: helper.service_type,
            host: helper.host,
            port: helper.port,
            user: helper.user,
            password: helper.password,
            database: helper.database,
            key_path: helper.key_path,
            tunnel_type: helper.tunnel_type,
            ssh_jump: helper.ssh_jump,
            ssh_user: helper.ssh_user,
            ssh_password: helper.ssh_password,
            ssh_key_path: helper.ssh_key_path,
            ssh_port: helper.ssh_port,
        })
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test test_load_toml_config`
Expected: PASS

- [ ] **Step 6: Create test fixtures**

Create: `tests/fixtures/test-config.toml`

```toml
[profiles.test-mysql]
type = "mysql"
host = "localhost"
port = 3306
user = "root"
password = "secret"
database = "testdb"
```

Create: `tests/fixtures/test-config.yaml`

```yaml
type: mysql
host: localhost
port: 3306
user: root
password: secret
database: testdb
```

- [ ] **Step 7: Commit**

```bash
git add src/config/loader.rs src/config/types.rs tests/fixtures/
git commit -m "feat: add configuration loader

- Implement ConfigLoader for TOML and YAML files
- Add Config deserialization support
- Create test fixtures

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Configuration Merger

**Files:**
- Create: `src/config/merger.rs`

- [ ] **Step 1: Write test for config merging**

Create: `src/config/merger.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ServiceType};

    #[test]
    fn test_merge_configs() {
        let base = Config {
            host: Some("base-host".to_string()),
            port: Some(3306),
            user: Some("base-user".to_string()),
            ..Default::default()
        };

        let override_cfg = Config {
            host: Some("override-host".to_string()),
            password: Some("override-pass".to_string()),
            ..Default::default()
        };

        let merged = ConfigMerger::merge(base, override_cfg);
        assert_eq!(merged.host.as_deref(), Some("override-host"));
        assert_eq!(merged.port, Some(3306));
        assert_eq!(merged.user.as_deref(), Some("base-user"));
        assert_eq!(merged.password.as_deref(), Some("override-pass"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_merge_configs`
Expected: FAIL with "ConfigMerger not defined"

- [ ] **Step 3: Implement ConfigMerger**

Add to top of `src/config/merger.rs`:

```rust
use crate::config::Config;

pub struct ConfigMerger;

impl ConfigMerger {
    pub fn merge(base: Config, override_cfg: Config) -> Config {
        Config {
            service_type: override_cfg.service_type.or(base.service_type),
            host: override_cfg.host.or(base.host),
            port: override_cfg.port.or(base.port),
            user: override_cfg.user.or(base.user),
            password: override_cfg.password.or(base.password),
            database: override_cfg.database.or(base.database),
            key_path: override_cfg.key_path.or(base.key_path),
            tunnel_type: override_cfg.tunnel_type.or(base.tunnel_type),
            ssh_jump: override_cfg.ssh_jump.or(base.ssh_jump),
            ssh_user: override_cfg.ssh_user.or(base.ssh_user),
            ssh_password: override_cfg.ssh_password.or(base.ssh_password),
            ssh_key_path: override_cfg.ssh_key_path.or(base.ssh_key_path),
            ssh_port: override_cfg.ssh_port.or(base.ssh_port),
        }
    }

    pub fn merge_multiple(configs: Vec<Config>) -> Config {
        configs.into_iter().fold(Config::default(), Self::merge)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_merge_configs`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config/merger.rs
git commit -m "feat: add configuration merger

- Implement ConfigMerger for priority-based merging
- Support merging multiple configs

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: CLI Arguments

**Files:**
- Create: `src/cli/mod.rs`
- Create: `src/cli/args.rs`

- [ ] **Step 1: Write test for CLI args parsing**

Create: `src/cli/args.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mysql_command() {
        let args = Cli::try_parse_from(&[
            "tools-mcp",
            "mysql",
            "SELECT 1",
            "--host=localhost",
            "--user=root",
        ]).unwrap();

        match args.command {
            Some(Commands::Mysql { query, host, user, .. }) => {
                assert_eq!(query, "SELECT 1");
                assert_eq!(host, Some("localhost".to_string()));
                assert_eq!(user, Some("root".to_string()));
            }
            _ => panic!("Expected Mysql command"),
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_parse_mysql_command`
Expected: FAIL with "Cli not defined"

- [ ] **Step 3: Implement CLI argument structures**

Add to top of `src/cli/args.rs`:

```rust
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TunnelType {
    Direct,
    Ssh,
}

#[derive(Args, Debug, Clone)]
pub struct TunnelArgs {
    #[arg(long, value_enum, default_value = "direct", help = "Tunnel type")]
    pub tunnel: TunnelType,

    #[arg(long, help = "SSH jump host (when --tunnel=ssh)")]
    pub ssh_jump: Option<String>,

    #[arg(long, help = "SSH jump user (when --tunnel=ssh)")]
    pub ssh_user: Option<String>,

    #[arg(long, help = "SSH jump password (when --tunnel=ssh)")]
    pub ssh_password: Option<String>,

    #[arg(long, help = "SSH jump key path (when --tunnel=ssh)")]
    pub ssh_key_path: Option<String>,

    #[arg(long, help = "SSH jump port (when --tunnel=ssh)", default_value = "22")]
    pub ssh_port: Option<u16>,
}

#[derive(Parser, Debug)]
#[command(name = "tools-mcp")]
#[command(about = "Unified tool for SSH, MySQL, Redis connections with MCP support")]
pub struct Cli {
    #[arg(long, global = true, help = "Path to YAML config file")]
    pub config: Option<String>,

    #[command(flatten)]
    pub tunnel_args: TunnelArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Mysql {
        #[arg(help = "SQL query to execute")]
        query: String,

        #[arg(long, help = "MySQL host")]
        host: Option<String>,

        #[arg(long, help = "MySQL port")]
        port: Option<u16>,

        #[arg(long, help = "MySQL user")]
        user: Option<String>,

        #[arg(long, help = "MySQL password")]
        password: Option<String>,

        #[arg(long, help = "Database name")]
        database: Option<String>,

        #[arg(long, help = "Profile name from config")]
        profile: Option<String>,
    },
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_parse_mysql_command`
Expected: PASS

- [ ] **Step 5: Create CLI module entry**

Create: `src/cli/mod.rs`

```rust
mod args;
mod handler;

pub use args::{Cli, Commands, TunnelArgs, TunnelType};
pub use handler::CliHandler;
```

- [ ] **Step 6: Commit**

```bash
git add src/cli/
git commit -m "feat: add CLI argument parsing

- Define Cli struct with global parameters
- Add MySQL subcommand with query and connection args
- Support --config, --ssh-jump and related flags

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Tunnel Trait and DirectTunnel

**Files:**
- Create: `src/tunnel/mod.rs`
- Create: `src/tunnel/traits.rs`
- Create: `src/tunnel/direct.rs`

- [ ] **Step 1: Define Tunnel trait and TunnelEndpoint**

Create: `src/tunnel/traits.rs`

```rust
use crate::error::Result;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct TunnelEndpoint {
    pub host: String,
    pub port: u16,
}

#[async_trait]
pub trait Tunnel: Send + Sync {
    async fn establish(&mut self) -> Result<TunnelEndpoint>;
    async fn close(&mut self) -> Result<()>;
    fn is_active(&self) -> bool;
}
```

- [ ] **Step 2: Write test for DirectTunnel**

Create: `src/tunnel/direct.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_direct_tunnel() {
        let mut tunnel = DirectTunnel::new("localhost".to_string(), 3306);
        assert!(!tunnel.is_active());
        
        let endpoint = tunnel.establish().await.unwrap();
        assert_eq!(endpoint.host, "localhost");
        assert_eq!(endpoint.port, 3306);
        assert!(tunnel.is_active());
        
        tunnel.close().await.unwrap();
        assert!(!tunnel.is_active());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test test_direct_tunnel`
Expected: FAIL with "DirectTunnel not defined"

- [ ] **Step 4: Implement DirectTunnel**

Add to top of `src/tunnel/direct.rs`:

```rust
use crate::tunnel::traits::{Tunnel, TunnelEndpoint};
use crate::error::Result;
use async_trait::async_trait;

pub struct DirectTunnel {
    target_host: String,
    target_port: u16,
    active: bool,
}

impl DirectTunnel {
    pub fn new(target_host: String, target_port: u16) -> Self {
        Self {
            target_host,
            target_port,
            active: false,
        }
    }
}

#[async_trait]
impl Tunnel for DirectTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        self.active = true;
        Ok(TunnelEndpoint {
            host: self.target_host.clone(),
            port: self.target_port,
        })
    }

    async fn close(&mut self) -> Result<()> {
        self.active = false;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test test_direct_tunnel`
Expected: PASS

- [ ] **Step 6: Create tunnel module entry**

Create: `src/tunnel/mod.rs`

```rust
mod traits;
mod direct;

pub use traits::{Tunnel, TunnelEndpoint};
pub use direct::DirectTunnel;
```

- [ ] **Step 7: Commit**

```bash
git add src/tunnel/
git commit -m "feat: add Tunnel trait and DirectTunnel

- Define Tunnel trait for extensible tunnel implementations
- Implement DirectTunnel for direct connections (no tunneling)
- Add TunnelEndpoint for connection endpoints

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: MySQL Connection with Tunnel Support

**Files:**
- Create: `src/connection/mod.rs`
- Create: `src/connection/traits.rs`
- Create: `src/connection/mysql.rs`

- [ ] **Step 1: Define Connection trait**

Create: `src/connection/traits.rs`

```rust
use crate::error::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Connection: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
}
```

- [ ] **Step 2: Add async-trait dependency**

Add to `Cargo.toml` dependencies:

```toml
async-trait = "0.1"
```

- [ ] **Step 3: Write test for MySQL connection**

Create: `src/connection/mysql.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnel::DirectTunnel;

    #[tokio::test]
    async fn test_mysql_connection_new() {
        let tunnel = Box::new(DirectTunnel::new("localhost".to_string(), 3306));
        let conn = MySQLConnection::new(
            tunnel,
            "root".to_string(),
            Some("password".to_string()),
            None,
        );
        assert!(!conn.is_connected());
    }
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test test_mysql_connection_new`
Expected: FAIL with "MySQLConnection not defined"

- [ ] **Step 5: Implement MySQLConnection**

Add to top of `src/connection/mysql.rs`:

```rust
use crate::connection::traits::Connection;
use crate::error::{Error, Result};
use crate::tunnel::Tunnel;
use async_trait::async_trait;
use mysql_async::{Conn, OptsBuilder, Pool};

pub struct MySQLConnection {
    tunnel: Box<dyn Tunnel>,
    user: String,
    password: Option<String>,
    database: Option<String>,
    pool: Option<Pool>,
    conn: Option<Conn>,
}

impl MySQLConnection {
    pub fn new(
        tunnel: Box<dyn Tunnel>,
        user: String,
        password: Option<String>,
        database: Option<String>,
    ) -> Self {
        Self {
            tunnel,
            user,
            password,
            database,
            pool: None,
            conn: None,
        }
    }

    pub async fn get_conn(&mut self) -> Result<&mut Conn> {
        if self.conn.is_none() {
            self.connect().await?;
        }
        self.conn.as_mut().ok_or_else(|| {
            Error::Connection("Connection not established".to_string())
        })
    }
}

#[async_trait]
impl Connection for MySQLConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;
        
        let mut opts = OptsBuilder::default()
            .ip_or_hostname(&endpoint.host)
            .tcp_port(endpoint.port)
            .user(Some(&self.user));

        if let Some(ref password) = self.password {
            opts = opts.pass(Some(password));
        }

        if let Some(ref database) = self.database {
            opts = opts.db_name(Some(database));
        }

        let pool = Pool::new(opts);
        let conn = pool.get_conn().await?;
        
        self.pool = Some(pool);
        self.conn = Some(conn);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
        if let Some(pool) = self.pool.take() {
            pool.disconnect().await?;
        }
        self.tunnel.close().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.conn.is_some()
    }
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test test_mysql_connection_new`
Expected: PASS

- [ ] **Step 7: Create connection module entry**

Create: `src/connection/mod.rs`

```rust
mod traits;
mod mysql;

pub use traits::Connection;
pub use mysql::MySQLConnection;
```

- [ ] **Step 8: Commit**

```bash
git add src/connection/ Cargo.toml
git commit -m "feat: add MySQL connection with Tunnel support

- Define Connection trait with async methods
- Implement MySQLConnection using Tunnel abstraction
- Connection uses tunnel.establish() to get endpoint
- Add async-trait dependency

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: MySQL Executor

**Files:**
- Create: `src/executor/mod.rs`
- Create: `src/executor/mysql.rs`
- Create: `src/output/mod.rs`
- Create: `src/output/types.rs`

- [ ] **Step 1: Define output types**

Create: `src/output/types.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub affected_rows: u64,
}

impl ExecutionResult {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<String>>, affected_rows: u64) -> Self {
        Self {
            columns,
            rows,
            affected_rows,
        }
    }
}
```

- [ ] **Step 2: Write test for MySQL executor**

Create: `src/executor/mysql.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_executor_new() {
        let executor = MySQLExecutor;
        assert!(true); // Placeholder test
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test test_mysql_executor_new`
Expected: FAIL with "MySQLExecutor not defined"

- [ ] **Step 4: Implement MySQLExecutor**

Add to top of `src/executor/mysql.rs`:

```rust
use crate::connection::MySQLConnection;
use crate::error::Result;
use crate::output::ExecutionResult;
use mysql_async::{prelude::*, Row, Value};

pub struct MySQLExecutor;

impl MySQLExecutor {
    pub async fn execute(
        conn: &mut MySQLConnection,
        query: &str,
    ) -> Result<ExecutionResult> {
        let mysql_conn = conn.get_conn().await?;
        
        let result: Vec<Row> = mysql_conn.query(query).await?;
        
        if result.is_empty() {
            return Ok(ExecutionResult::new(vec![], vec![], 0));
        }

        let columns: Vec<String> = result[0]
            .columns()
            .iter()
            .map(|col| col.name_str().to_string())
            .collect();

        let rows: Vec<Vec<String>> = result
            .iter()
            .map(|row| {
                row.unwrap_ref()
                    .iter()
                    .map(|value| Self::value_to_string(value))
                    .collect()
            })
            .collect();

        let affected_rows = rows.len() as u64;

        Ok(ExecutionResult::new(columns, rows, affected_rows))
    }

    fn value_to_string(value: &Value) -> String {
        match value {
            Value::NULL => "NULL".to_string(),
            Value::Bytes(b) => String::from_utf8_lossy(b).to_string(),
            Value::Int(i) => i.to_string(),
            Value::UInt(u) => u.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Double(d) => d.to_string(),
            Value::Date(y, m, d, h, min, s, _) => {
                format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, s)
            }
            Value::Time(_, _, h, m, s, _) => format!("{:02}:{:02}:{:02}", h, m, s),
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test test_mysql_executor_new`
Expected: PASS

- [ ] **Step 6: Create module entry points**

Create: `src/executor/mod.rs`

```rust
mod mysql;

pub use mysql::MySQLExecutor;
```

Create: `src/output/mod.rs`

```rust
mod types;
mod cli;

pub use types::ExecutionResult;
pub use cli::CliFormatter;
```

- [ ] **Step 7: Commit**

```bash
git add src/executor/ src/output/types.rs src/output/mod.rs
git commit -m "feat: add MySQL executor and output types

- Implement MySQLExecutor for query execution
- Define ExecutionResult for structured output
- Add value-to-string conversion for MySQL types

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: CLI Output Formatter

**Files:**
- Create: `src/output/cli.rs`

- [ ] **Step 1: Write test for CLI formatter**

Create: `src/output/cli.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::ExecutionResult;

    #[test]
    fn test_format_table() {
        let result = ExecutionResult::new(
            vec!["id".to_string(), "name".to_string()],
            vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ],
            2,
        );

        let output = CliFormatter::format(&result);
        assert!(output.contains("id"));
        assert!(output.contains("name"));
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_format_table`
Expected: FAIL with "CliFormatter not defined"

- [ ] **Step 3: Implement CliFormatter**

Add to top of `src/output/cli.rs`:

```rust
use crate::output::ExecutionResult;
use comfy_table::{Table, presets::UTF8_FULL};

pub struct CliFormatter;

impl CliFormatter {
    pub fn format(result: &ExecutionResult) -> String {
        if result.rows.is_empty() {
            return format!("Query OK, {} rows affected", result.affected_rows);
        }

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(&result.columns);

        for row in &result.rows {
            table.add_row(row);
        }

        format!("{}\n\n{} rows in set", table, result.affected_rows)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_format_table`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/output/cli.rs
git commit -m "feat: add CLI output formatter

- Implement CliFormatter with table formatting
- Use comfy-table for pretty output
- Show row count after results

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: CLI Handler

**Files:**
- Create: `src/cli/handler.rs`

- [ ] **Step 1: Write test for CLI handler**

Create: `src/cli/handler.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_handler_new() {
        let handler = CliHandler;
        assert!(true); // Placeholder test
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_cli_handler_new`
Expected: FAIL with "CliHandler not defined"

- [ ] **Step 3: Implement CliHandler**

Add to top of `src/cli/handler.rs`:

```rust
use crate::cli::{Cli, Commands, TunnelType};
use crate::config::{Config, ConfigLoader, ConfigMerger, ServiceType};
use crate::connection::MySQLConnection;
use crate::error::{Error, Result};
use crate::executor::MySQLExecutor;
use crate::output::CliFormatter;
use crate::tunnel::{DirectTunnel, Tunnel};
use std::path::Path;

pub struct CliHandler;

impl CliHandler {
    pub async fn handle(cli: Cli) -> Result<()> {
        match cli.command {
            Some(Commands::Mysql {
                query,
                host,
                port,
                user,
                password,
                database,
                profile,
            }) => {
                let config = Self::build_config(
                    &cli,
                    ServiceType::Mysql,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None,
                    profile,
                )?;

                Self::execute_mysql(&query, config).await
            }
            None => {
                Err(Error::Config("No command specified. Run with --help for usage.".to_string()))
            }
        }
    }

    fn build_config(
        cli: &Cli,
        service_type: ServiceType,
        host: Option<String>,
        port: Option<u16>,
        user: Option<String>,
        password: Option<String>,
        database: Option<String>,
        key_path: Option<String>,
        profile: Option<String>,
    ) -> Result<Config> {
        let mut configs = vec![];

        // 1. Load default TOML config
        if let Some(toml_config) = ConfigLoader::load_default_toml()? {
            if let Some(profile_name) = &profile {
                if let Some(profile_cfg) = toml_config.profiles.get(profile_name) {
                    configs.push(Self::profile_to_config(profile_cfg));
                }
            }
        }

        // 2. Load YAML config if specified
        if let Some(config_path) = &cli.config {
            let yaml_config = ConfigLoader::load_yaml_file(Path::new(config_path))?;
            configs.push(yaml_config);
        }

        // 3. Add CLI arguments
        let tunnel_type = Self::cli_tunnel_to_config_tunnel(cli.tunnel_args.tunnel);
        let cli_config = Config {
            service_type: Some(service_type),
            host,
            port,
            user,
            password,
            database,
            key_path,
            tunnel_type: Some(tunnel_type),
            ssh_jump: cli.tunnel_args.ssh_jump.clone(),
            ssh_user: cli.tunnel_args.ssh_user.clone(),
            ssh_password: cli.tunnel_args.ssh_password.clone(),
            ssh_key_path: cli.tunnel_args.ssh_key_path.clone(),
            ssh_port: cli.tunnel_args.ssh_port,
        };
        configs.push(cli_config);

        Ok(ConfigMerger::merge_multiple(configs))
    }

    fn cli_tunnel_to_config_tunnel(cli_tunnel: TunnelType) -> crate::config::TunnelType {
        match cli_tunnel {
            TunnelType::Direct => crate::config::TunnelType::Direct,
            TunnelType::Ssh => crate::config::TunnelType::Ssh,
        }
    }

    fn profile_to_config(profile: &crate::config::Profile) -> Config {
        Config {
            service_type: Some(profile.service_type.clone()),
            host: profile.host.clone(),
            port: profile.port,
            user: profile.user.clone(),
            password: profile.password.clone(),
            database: profile.database.clone(),
            key_path: profile.key_path.clone(),
            tunnel_type: profile.tunnel_type.clone(),
            ssh_jump: profile.ssh_jump.clone(),
            ssh_user: profile.ssh_user.clone(),
            ssh_password: profile.ssh_password.clone(),
            ssh_key_path: profile.ssh_key_path.clone(),
            ssh_port: profile.ssh_port,
        }
    }

    async fn execute_mysql(query: &str, config: Config) -> Result<()> {
        let host = config.host.ok_or_else(|| {
            Error::Config("MySQL host is required".to_string())
        })?;
        let port = config.port.unwrap_or(3306);
        let user = config.user.ok_or_else(|| {
            Error::Config("MySQL user is required".to_string())
        })?;

        // Create tunnel based on config
        let tunnel: Box<dyn Tunnel> = Box::new(DirectTunnel::new(host, port));

        let mut conn = MySQLConnection::new(
            tunnel,
            user,
            config.password,
            config.database,
        );

        let result = MySQLExecutor::execute(&mut conn, query).await?;
        let output = CliFormatter::format(&result);
        println!("{}", output);

        conn.disconnect().await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_cli_handler_new`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/cli/handler.rs
git commit -m "feat: add CLI handler with config merging

- Implement CliHandler for command routing
- Build config from TOML profile, YAML file, and CLI args
- Execute MySQL queries with proper config priority

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: Main Entry Point

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write main.rs**

Replace contents of `src/main.rs`:

```rust
use clap::Parser;
use tools_mcp::cli::{Cli, CliHandler};
use tools_mcp::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.command.is_none() {
        println!("MCP mode not yet implemented. Use a subcommand (mysql) for CLI mode.");
        std::process::exit(1);
    }

    CliHandler::handle(cli).await
}
```

- [ ] **Step 2: Build the project**

Run: `cargo build`
Expected: Compilation succeeds

- [ ] **Step 3: Test with help command**

Run: `cargo run -- --help`
Expected: Shows help text with mysql subcommand

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add main entry point

- Parse CLI arguments with clap
- Route to CliHandler for command execution
- Placeholder message for MCP mode

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: Integration Test

**Files:**
- Create: `tests/integration/config_tests.rs`

- [ ] **Step 1: Create integration test for config loading**

Create: `tests/integration/config_tests.rs`

```rust
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tools_mcp::config::{ConfigLoader, ServiceType};

#[test]
fn test_load_yaml_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test.yaml");

    let yaml_content = r#"
type: mysql
host: localhost
port: 3306
user: root
password: secret
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let config = ConfigLoader::load_yaml_file(&config_path).unwrap();
    assert_eq!(config.service_type, Some(ServiceType::Mysql));
    assert_eq!(config.host.as_deref(), Some("localhost"));
    assert_eq!(config.port, Some(3306));
    assert_eq!(config.user.as_deref(), Some("root"));
}

#[test]
fn test_load_toml_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test.toml");

    let toml_content = r#"
[profiles.test]
type = "mysql"
host = "localhost"
port = 3306
user = "root"
"#;

    fs::write(&config_path, toml_content).unwrap();

    let config = ConfigLoader::load_toml_file(&config_path).unwrap();
    let profile = config.profiles.get("test").unwrap();
    assert_eq!(profile.service_type, ServiceType::Mysql);
    assert_eq!(profile.host.as_deref(), Some("localhost"));
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test config_tests`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add tests/integration/
git commit -m "test: add integration tests for config loading

- Test YAML config file loading
- Test TOML config file loading
- Use tempfile for isolated test fixtures

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: Documentation

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write README**

Replace contents of `README.md`:

```markdown
# tools-mcp

Unified tool for SSH, MySQL, and Redis connections with MCP (Model Context Protocol) support.

## Features

- **CLI Mode**: Execute commands directly from the command line
- **MCP Mode**: Run as an MCP server for AI assistant integration (coming soon)
- **Configuration**: Support for TOML profiles and YAML config files
- **SSH Jump Host**: Access internal services through bastion hosts (coming soon)

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
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add README with usage examples

- Document CLI usage for MySQL
- Show YAML and TOML config examples
- Add installation and development instructions

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: Final Verification

**Files:**
- All project files

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 2: Build release binary**

Run: `cargo build --release`
Expected: Binary created at `target/release/tools-mcp`

- [ ] **Step 3: Test CLI help**

Run: `./target/release/tools-mcp --help`
Expected: Shows usage information

- [ ] **Step 4: Test MySQL subcommand help**

Run: `./target/release/tools-mcp mysql --help`
Expected: Shows MySQL-specific options

- [ ] **Step 5: Create final commit**

```bash
git add -A
git commit -m "chore: Phase 1 complete - MySQL CLI with config support

Phase 1 deliverables:
- Project structure with modular design
- Configuration management (TOML + YAML)
- MySQL connection and query execution
- CLI mode with table-formatted output
- Integration tests

Next: Phase 2 will add SSH support and tunneling

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Summary

This plan implements Phase 1 of tools-mcp with:

1. **Project Setup**: Dependencies, error types, module structure
2. **Configuration System**: TOML profiles, YAML files, priority merging
3. **MySQL Support**: Connection, query execution, result formatting
4. **CLI Interface**: Argument parsing, command routing, output formatting
5. **Testing**: Unit tests and integration tests
6. **Documentation**: README with usage examples

**What's Working:**
- `tools-mcp mysql "SELECT 1" --host=localhost --user=root`
- `tools-mcp --config=mysql.yaml mysql "SELECT * FROM users"`
- `tools-mcp mysql "SELECT * FROM users" --profile=prod`

**Not Yet Implemented (Future Phases):**
- SSH direct connection
- SSH tunneling for MySQL/Redis
- Redis support
- MCP server mode

