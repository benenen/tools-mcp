//! Top-level entry: build (optional) SSH jump chain, open final SSH
//! session to the target with target credentials, exec the command,
//! map the output to an ExecutionResult.

use russh::client;
use std::sync::Arc;
use tokio::sync::Mutex;
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::exec::{SshExec, output_to_result};
use crate::request::{SshExecRequest, SshJumpsConfig};
use crate::session::{AcceptAnyHostKey, authenticate, build_session_chain};

/// Run a single shell command on the SSH target described by `req`,
/// optionally going through `jumps`. Always tears down the chain via Drop
/// before returning.
pub async fn execute(
    req: SshExecRequest,
    jumps: Option<SshJumpsConfig>,
) -> Result<ExecutionResult> {
    let cfg = std::sync::Arc::new(client::Config::default());

    // Build the jump chain (if any). Returns the last jump's session.
    let mut jump_sessions = match &jumps {
        Some(j) if !j.jumps.is_empty() => {
            build_session_chain(
                &j.jumps,
                &j.user,
                j.password.as_deref(),
                j.key_path.as_deref(),
                j.port,
            )
            .await?
        }
        _ => Vec::new(),
    };

    // Open the FINAL SSH session to the target. If we have a jump chain,
    // open a direct-tcpip channel from the last jump and run SSH over it
    // (with TARGET's credentials, not the jump credentials). If we don't,
    // TCP-connect directly.
    let target_handler = AcceptAnyHostKey {
        label: req.host.clone(),
    };
    let mut target_session = if let Some(last_jump) = jump_sessions.last() {
        let channel = last_jump
            .lock()
            .await
            .channel_open_direct_tcpip(req.host.clone(), req.port as u32, "127.0.0.1", 0u32)
            .await
            .map_err(|e| {
                Error::Connection(format!(
                    "open direct-tcpip to {}:{} via last jump failed: {e}",
                    req.host, req.port
                ))
            })?;
        let stream = Box::pin(channel.into_stream());
        client::connect_stream(cfg, stream, target_handler)
            .await
            .map_err(|e| {
                Error::Connection(format!(
                    "SSH connect to {} (chained) failed: {e}",
                    req.host
                ))
            })?
    } else {
        client::connect(cfg, (req.host.as_str(), req.port), target_handler)
            .await
            .map_err(|e| Error::Connection(format!("SSH connect to {} failed: {e}", req.host)))?
    };

    // Authenticate with TARGET's creds (not the jump creds).
    authenticate(
        &mut target_session,
        &req.user,
        req.password.as_deref(),
        req.key_path.as_deref(),
    )
    .await?;

    let target_session = Arc::new(Mutex::new(target_session));

    // Exec the command.
    let result = SshExec::run(target_session.clone(), &req.command).await;

    // Drop the target session and the jump chain (Drop closes the
    // underlying channels/connections).
    drop(target_session);
    jump_sessions.clear();

    Ok(output_to_result(result?))
}
