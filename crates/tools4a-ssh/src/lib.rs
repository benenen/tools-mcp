//! SSH stack: command-exec primitives, the `SshDirectOrchestrator`
//! `Service` impl, and the `SshMcp` `McpTool` impl. Session-chain
//! helpers (`AcceptAnyHostKey`, `authenticate`, `build_session_chain`)
//! live in `tools4a-core` so `core::SshTunnel` can also use them.

pub mod exec;
pub mod execute;
pub mod mcp;
pub mod orchestrator;
pub mod request;

pub use exec::{SshExec, SshOutput, output_to_result};
pub use execute::execute;
pub use mcp::{SshExecParams, SshMcp};
pub use orchestrator::SshDirectOrchestrator;
pub use request::{SshExecRequest, SshJumpsConfig};
