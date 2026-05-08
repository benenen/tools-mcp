//! SSH session-chain primitives (shared by `tools-mcp` bin's SshTunnel and
//! this crate's SshExec) plus a top-level `execute()` function for running
//! a single shell command on an SSH target, optionally through one or more
//! jump hosts.

pub mod session;

pub use session::{AcceptAnyHostKey, authenticate, build_session_chain};
