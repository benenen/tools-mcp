//! SSH-direct orchestrator: typed `SshExecRequest` → `tools4a_ssh::execute`
//! with optional jump-host chain built from the tunnel config.
//!
//! Like HTTP, SSH-direct doesn't have a `from_config` constructor —
//! Profile/YAML support was deferred (Phase 7 simplification). The bin
//! builds `SshExecRequest` directly from CLI flags / JSON params and
//! calls `SshDirectOrchestrator::execute(req, tunnel)`.
//!
//! Validation (`req.password` or `req.key_path` must be set) lives in
//! the orchestrator since there's no `from_config` consolidation point.

use async_trait::async_trait;
use tools4a_core::{Error, ExecutionResult, Result, Service, TunnelConfig};
use tools4a_ssh::{SshExecRequest, SshJumpsConfig, execute as ssh_execute};

pub struct SshDirectOrchestrator;

#[async_trait]
impl Service for SshDirectOrchestrator {
    type Request = SshExecRequest;

    async fn execute(
        req: SshExecRequest,
        tunnel_config: Option<TunnelConfig>,
    ) -> Result<ExecutionResult> {
        if req.password.is_none() && req.key_path.is_none() {
            return Err(Error::Config(
                "SSH target requires --password or --key-path".to_string(),
            ));
        }

        let jumps = match tunnel_config {
            None | Some(TunnelConfig::Direct) => None,
            Some(TunnelConfig::Ssh {
                ssh_jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
            }) => Some(SshJumpsConfig {
                jumps: ssh_jumps,
                user: ssh_user,
                password: ssh_password,
                key_path: ssh_key_path.map(std::path::PathBuf::from),
                port: ssh_port,
            }),
        };

        ssh_execute(req, jumps).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_req() -> SshExecRequest {
        SshExecRequest {
            host: "h".to_string(),
            port: 22,
            user: "u".to_string(),
            password: None,
            key_path: None,
            command: "ls".to_string(),
        }
    }

    #[tokio::test]
    async fn test_execute_errors_without_password_or_key() {
        let err = SshDirectOrchestrator::execute(empty_req(), None)
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::Config(msg) if msg.contains("--password or --key-path")),
            "expected Config error about missing creds"
        );
    }
}
