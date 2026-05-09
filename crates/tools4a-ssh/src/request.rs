//! SSH-direct request input shape — independent of any caller (CLI, MCP).

/// Resolved SSH-exec request to execute. Caller (CLI handler / MCP tool)
/// builds this from the user's flags / JSON params; the lib doesn't care
/// where the fields came from.
#[derive(Debug, Clone)]
pub struct SshExecRequest {
    /// Target SSH host (the machine where `command` runs).
    pub host: String,
    /// Target SSH port (default 22).
    pub port: u16,
    /// Target SSH user.
    pub user: String,
    /// Target SSH password (mutually exclusive with key_path; at least one
    /// of password / key_path must be provided).
    pub password: Option<String>,
    /// Path to an unencrypted private key file (passphrase-protected keys
    /// are not supported in this phase).
    pub key_path: Option<std::path::PathBuf>,
    /// Shell command to execute on the target.
    pub command: String,
}

/// Optional SSH-jump config: a chain of bastion hosts plus the credentials
/// to authenticate to ALL of them (per-hop overrides aren't supported yet).
/// When `None` is passed to `execute`, the target SSH server is reached
/// directly.
#[derive(Debug, Clone)]
pub struct SshJumpsConfig {
    pub jumps: Vec<String>,
    pub user: String,
    pub password: Option<String>,
    pub key_path: Option<std::path::PathBuf>,
    pub port: u16,
}
