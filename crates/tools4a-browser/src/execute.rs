//! Top-level entry: run one agent-browser invocation and return the
//! structured result. No tunnel handling here — Phase 1 only supports
//! direct execution; the orchestrator validates `TunnelConfig::Ssh`
//! is not set and surfaces a Phase 2 deferral message.

use tools4a_core::{ExecutionResult, Result};

use crate::exec::{BrowserExec, output_to_result};
use crate::request::BrowserRequest;

pub async fn execute(req: BrowserRequest) -> Result<ExecutionResult> {
    let out = BrowserExec::run(req).await?;
    Ok(output_to_result(out))
}
