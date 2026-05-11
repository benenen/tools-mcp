//! Core traits and shared types for the tools4a workspace.
//!
//! Holds the trait floor (`Tunnel`, `Connection`, `Service`, `McpTool`),
//! shared error/result types, the `TunnelConfig` enum, the
//! Profile/YAML/CLI 3-layer Config types, the concrete `DirectTunnel`
//! and `SshTunnel` runtime impls, and the SSH `session` helpers shared
//! between `SshTunnel` and `tools4a-ssh`'s `SshExec`. Per-service
//! orchestrator + MCP impls live in their leaf crate (`tools4a-mysql`,
//! `tools4a-pgsql`, …).

pub mod config;
pub mod mcp;
pub mod readonly;
pub mod session;
pub mod timeout;
pub mod tunnel;

pub use mcp::{McpTool, SshJumpInput, TunnelKind, build_tunnel_config};
pub use timeout::{
    DEFAULT_MAX_TIMEOUT_SECS, EffectiveTimeout, apply_with_timeout, resolve_effective_timeout,
};
pub use tunnel::{DirectTunnel, SshTunnel, build_tunnel};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

// -- Error --------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    Config(String),
    Connection(String),
    Execution(String),
    Io(std::io::Error),
    /// Errors from a specific service (MySQL, SSH library, YAML parser, …).
    /// Higher crates wrap their library errors into this variant via
    /// `Error::Service(format!("{e}"))` to keep core dep-free.
    Service(String),
    /// The underlying protocol call exceeded the resolved timeout.
    /// Carries the full `EffectiveTimeout` so the error message can
    /// distinguish "you asked for 60s and got 60s" from "you asked for
    /// 60s but the operator-side cap shrank it to 3s".
    Timeout(timeout::EffectiveTimeout),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(msg) => write!(f, "Configuration error: {msg}"),
            Error::Connection(msg) => write!(f, "Connection error: {msg}"),
            Error::Execution(msg) => write!(f, "Execution error: {msg}"),
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Service(msg) => write!(f, "Service error: {msg}"),
            Error::Timeout(t) => {
                if t.clamped {
                    write!(
                        f,
                        "Timeout: operation exceeded {}s (requested {}s was capped to the {}s ceiling)",
                        t.effective_secs, t.requested_secs, t.max_secs
                    )
                } else {
                    write!(f, "Timeout: operation exceeded {}s", t.effective_secs)
                }
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Config(_)
            | Error::Connection(_)
            | Error::Execution(_)
            | Error::Service(_)
            | Error::Timeout(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// -- Tunnel -------------------------------------------------------------

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

// -- Connection ---------------------------------------------------------

#[async_trait]
pub trait Connection: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
}

// -- ExecutionResult ----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub affected_rows: u64,
    /// Non-fatal advisories surfaced by the orchestrator (e.g. a clamp
    /// notice when the requested timeout exceeded the configured maximum).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl ExecutionResult {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<String>>, affected_rows: u64) -> Self {
        Self {
            columns,
            rows,
            affected_rows,
            warnings: Vec::new(),
        }
    }

    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    pub fn push_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

// -- TunnelConfig -------------------------------------------------------

/// Tunnel selection plus its parameters. Shared shape across all services.
/// Runtime impls (`DirectTunnel`, `SshTunnel`) live in this crate's
/// `tunnel` module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TunnelConfig {
    Direct,
    Ssh {
        /// One or more jump hosts in client→target order. YAML/TOML accepts
        /// either a single string (legacy single-hop) or a sequence of strings.
        #[serde(rename = "ssh_jump", deserialize_with = "deserialize_string_or_vec")]
        ssh_jumps: Vec<String>,
        ssh_user: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ssh_password: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ssh_key_path: Option<String>,
        #[serde(default = "default_ssh_port")]
        ssh_port: u16,
    },
}

fn default_ssh_port() -> u16 {
    22
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }
    match StringOrVec::deserialize(deserializer)? {
        StringOrVec::String(s) => Ok(vec![s]),
        StringOrVec::Vec(v) => Ok(v),
    }
}

// -- Service trait ------------------------------------------------------

/// A service orchestrator: takes a typed request + an optional tunnel
/// config, returns a structured result. Each leaf service crate
/// (`tools4a-mysql`, `tools4a-pgsql`, …) implements this for its own
/// `<Svc>Orchestrator` type. CLI/MCP layers build the typed request
/// (resolving Profile/YAML/CLI args before this point) and dispatch.
#[async_trait]
pub trait Service {
    /// Service-specific request shape. CLI handler / MCP tool builds
    /// this from user input.
    type Request;

    async fn execute(req: Self::Request, tunnel: Option<TunnelConfig>) -> Result<ExecutionResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tunnel_config_ssh_accepts_string_for_jump() {
        let yaml = r#"
type: ssh
ssh_jump: bastion.com
ssh_user: admin
"#;
        let cfg: TunnelConfig = serde_yml::from_str(yaml).unwrap();
        match cfg {
            TunnelConfig::Ssh {
                ssh_jumps,
                ssh_user,
                ..
            } => {
                assert_eq!(ssh_jumps, vec!["bastion.com".to_string()]);
                assert_eq!(ssh_user, "admin");
            }
            _ => panic!("expected Ssh"),
        }
    }

    #[test]
    fn test_tunnel_config_ssh_accepts_array_for_jump() {
        let yaml = r#"
type: ssh
ssh_jump:
  - bastion1.com
  - bastion2.com
ssh_user: admin
"#;
        let cfg: TunnelConfig = serde_yml::from_str(yaml).unwrap();
        match cfg {
            TunnelConfig::Ssh { ssh_jumps, .. } => {
                assert_eq!(
                    ssh_jumps,
                    vec!["bastion1.com".to_string(), "bastion2.com".to_string()]
                );
            }
            _ => panic!("expected Ssh"),
        }
    }
}
