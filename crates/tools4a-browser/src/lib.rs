//! Browser stack: thin wrapper around the external `agent-browser`
//! CLI (https://github.com/vercel-labs/agent-browser). tools4a does
//! not embed a browser; the daemon spawned by agent-browser owns all
//! session / page / cookie state. Each call here is one short-lived
//! CLI invocation against that persistent daemon, captured as an
//! `ExecutionResult` (stdout / stderr / exit_code).

pub mod exec;
pub mod execute;
pub mod orchestrator;
pub mod request;

pub use exec::{BrowserExec, BrowserOutput, output_to_result, resolve_bin};
pub use execute::execute;
pub use orchestrator::BrowserOrchestrator;
pub use request::BrowserRequest;
