//! SSH stack: session-chain primitives, the `SshDirectOrchestrator`
//! `Service` impl, and the `SshMcp` `McpTool` impl. Session helpers
//! (`AcceptAnyHostKey`, `authenticate`, `build_session_chain`) are
//! exported so `tools4a-tunnel`'s `SshTunnel` can reuse them.

pub mod exec;
pub mod execute;
pub mod mcp;
pub mod orchestrator;
pub mod request;
pub mod session;

pub use exec::{SshExec, SshOutput, output_to_result};
pub use execute::execute;
pub use mcp::{SshExecParams, SshMcp};
pub use orchestrator::SshDirectOrchestrator;
pub use request::{SshExecRequest, SshJumpsConfig};
pub use session::{AcceptAnyHostKey, authenticate, build_session_chain};
