//! Open a session channel on a russh client, exec a command, collect
//! stdout/stderr/exit_code, and map into an `ExecutionResult`.

use russh::ChannelMsg;
use russh::client;
use std::sync::Arc;
use tokio::sync::Mutex;
use tools4a_core::session::AcceptAnyHostKey;
use tools4a_core::{Error, ExecutionResult, Result};

pub struct SshExec;

/// Stdout/stderr collected during exec, plus the remote exit code (if the
/// remote sent one — for clean exits this is always `Some`).
#[derive(Debug, Clone)]
pub struct SshOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<u32>,
}

impl SshExec {
    /// Open a `session` channel on `final_session`, exec `command`, and
    /// collect stdout/stderr/exit_code until the channel closes.
    pub async fn run(
        final_session: Arc<Mutex<client::Handle<AcceptAnyHostKey>>>,
        command: &str,
    ) -> Result<SshOutput> {
        let mut channel = final_session
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(|e| Error::Service(format!("SSH session open failed: {e}")))?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| Error::Service(format!("SSH exec request failed: {e}")))?;

        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let mut exit_code: Option<u32> = None;

        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.extend_from_slice(data);
                }
                ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                    stderr.extend_from_slice(data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(exit_status);
                }
                _ => {}
            }
        }

        Ok(SshOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
}

/// Map collected SSH output into an `ExecutionResult` with rows
/// `["exit_code", ...]`, `["stdout", ...]`, `["stderr", ...]`.
/// Bytes are UTF-8-decoded if possible; otherwise rendered as
/// `<N bytes (non-UTF-8)>`.
pub fn output_to_result(output: SshOutput) -> ExecutionResult {
    let stdout_cell = bytes_to_cell(&output.stdout);
    let stderr_cell = bytes_to_cell(&output.stderr);
    let exit_cell = match output.exit_code {
        Some(c) => c.to_string(),
        None => "<unknown>".to_string(),
    };

    let rows: Vec<Vec<String>> = vec![
        vec!["exit_code".to_string(), exit_cell],
        vec!["stdout".to_string(), stdout_cell],
        vec!["stderr".to_string(), stderr_cell],
    ];
    let affected = rows.len() as u64;
    ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        rows,
        affected,
    )
}

fn bytes_to_cell(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(text) => text.to_string(),
        Err(_) => format!("<{} bytes (non-UTF-8)>", b.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_to_result_utf8() {
        let out = SshOutput {
            stdout: b"hello\n".to_vec(),
            stderr: b"warn: something\n".to_vec(),
            exit_code: Some(0),
        };
        let r = output_to_result(out);
        assert_eq!(r.columns, vec!["field".to_string(), "value".to_string()]);
        assert_eq!(r.affected_rows, 3);
        assert_eq!(r.rows[0], vec!["exit_code".to_string(), "0".to_string()]);
        assert_eq!(r.rows[1], vec!["stdout".to_string(), "hello\n".to_string()]);
        assert_eq!(
            r.rows[2],
            vec!["stderr".to_string(), "warn: something\n".to_string()]
        );
    }

    #[test]
    fn test_output_to_result_non_utf8() {
        let out = SshOutput {
            stdout: vec![0xff, 0xfe, 0xfd],
            stderr: Vec::new(),
            exit_code: Some(127),
        };
        let r = output_to_result(out);
        assert_eq!(r.rows[0], vec!["exit_code".to_string(), "127".to_string()]);
        assert_eq!(
            r.rows[1],
            vec!["stdout".to_string(), "<3 bytes (non-UTF-8)>".to_string()]
        );
    }

    #[test]
    fn test_output_to_result_unknown_exit() {
        let out = SshOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: None,
        };
        let r = output_to_result(out);
        assert_eq!(
            r.rows[0],
            vec!["exit_code".to_string(), "<unknown>".to_string()]
        );
    }
}
