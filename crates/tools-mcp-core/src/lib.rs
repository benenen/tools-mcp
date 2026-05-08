//! Core traits and shared types for the tools-mcp workspace.
//!
//! This crate is the dependency floor: only `async-trait` and `serde`.
//! Service-specific code (MySQL, SSH, etc.) lives in higher crates.

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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(msg) => write!(f, "Configuration error: {msg}"),
            Error::Connection(msg) => write!(f, "Connection error: {msg}"),
            Error::Execution(msg) => write!(f, "Execution error: {msg}"),
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::Service(msg) => write!(f, "Service error: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Config(_) | Error::Connection(_) | Error::Execution(_) | Error::Service(_) => {
                None
            }
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
