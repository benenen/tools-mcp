//! SSH session-chain primitives (shared by `tools4a` bin's SshTunnel and
//! this crate's SshExec) plus a top-level `execute()` function for running
//! a single shell command on an SSH target, optionally through one or more
//! jump hosts.

pub mod exec;
pub mod execute;
pub mod request;
pub mod session;

pub use exec::{SshExec, SshOutput, output_to_result};
pub use execute::execute;
pub use request::{SshExecRequest, SshJumpsConfig};
pub use session::{AcceptAnyHostKey, authenticate, build_session_chain};
