//! Spawn the external `agent-browser` binary and capture
//! stdout / stderr / exit code into the standard `ExecutionResult`
//! shape. Modeled after `tools4a_ssh::exec` — same field layout so
//! MCP clients get a consistent shape across `ssh_exec` and
//! `browser_exec`.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;
use tools4a_core::{Error, ExecutionResult, Result};

use crate::request::BrowserRequest;

/// Captured output of one agent-browser invocation.
#[derive(Debug, Clone)]
pub struct BrowserOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Map a captured BrowserOutput to the standard 3-row ExecutionResult.
/// Layout matches `tools4a_ssh::output_to_result`.
pub fn output_to_result(out: BrowserOutput) -> ExecutionResult {
    let rows = vec![
        vec!["exit_code".to_string(), out.exit_code.to_string()],
        vec!["stdout".to_string(), out.stdout],
        vec!["stderr".to_string(), out.stderr],
    ];
    let affected = rows.len() as u64;
    ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        rows,
        affected,
    )
}

/// Resolve which binary to invoke.
///
/// Priority: explicit `req.bin` -> `$AGENT_BROWSER_BIN` env ->
/// `"agent-browser"` (let `Command` walk `$PATH`).
pub fn resolve_bin(req: &BrowserRequest) -> PathBuf {
    if let Some(p) = &req.bin {
        return p.clone();
    }
    if let Ok(s) = std::env::var("AGENT_BROWSER_BIN")
        && !s.is_empty()
    {
        return PathBuf::from(s);
    }
    PathBuf::from("agent-browser")
}

pub struct BrowserExec;

impl BrowserExec {
    pub async fn run(req: BrowserRequest) -> Result<BrowserOutput> {
        let bin = resolve_bin(&req);

        let mut cmd = Command::new(&bin);
        cmd.arg(&req.subcommand);
        for a in &req.args {
            cmd.arg(a);
        }
        if let Some(s) = &req.session {
            cmd.arg("--session").arg(s);
        }
        if let Some(p) = &req.proxy {
            cmd.arg("--proxy").arg(p);
        }
        if let Some(b) = &req.proxy_bypass {
            cmd.arg("--proxy-bypass").arg(b);
        }
        if let Some(a) = &req.browser_args {
            cmd.arg("--args").arg(a);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Config(format!(
                    "agent-browser binary not found at '{}'. Install with `npm i -g agent-browser` \
                     or the upstream Rust build, then ensure it's on $PATH (or set $AGENT_BROWSER_BIN). \
                     Upstream: https://github.com/vercel-labs/agent-browser",
                    bin.display()
                ))
            } else {
                Error::Service(format!("agent-browser spawn failed: {e}"))
            }
        })?;

        Ok(BrowserOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(sub: &str) -> BrowserRequest {
        BrowserRequest {
            subcommand: sub.into(),
            args: Vec::new(),
            session: None,
            proxy: None,
            proxy_bypass: None,
            browser_args: None,
            bin: None,
        }
    }

    #[test]
    fn resolve_bin_explicit_wins() {
        let r = BrowserRequest {
            bin: Some(PathBuf::from("/opt/ab")),
            ..req("open")
        };
        assert_eq!(resolve_bin(&r), PathBuf::from("/opt/ab"));
    }

    #[test]
    fn resolve_bin_env_used_when_no_explicit() {
        // SAFETY: these env-var tests are interdependent under cargo test's
        // default parallel execution. Acceptable because no other test in
        // the workspace reads AGENT_BROWSER_BIN; if that changes, gate
        // these with #[ignore] + --test-threads=1.
        unsafe {
            std::env::set_var("AGENT_BROWSER_BIN", "/etc/ab-from-env");
        }
        let got = resolve_bin(&req("open"));
        unsafe {
            std::env::remove_var("AGENT_BROWSER_BIN");
        }
        assert_eq!(got, PathBuf::from("/etc/ab-from-env"));
    }

    #[test]
    fn resolve_bin_falls_back_to_path_name() {
        unsafe {
            std::env::remove_var("AGENT_BROWSER_BIN");
        }
        assert_eq!(resolve_bin(&req("open")), PathBuf::from("agent-browser"));
    }

    #[test]
    fn output_to_result_layout() {
        let r = output_to_result(BrowserOutput {
            stdout: "hi\n".into(),
            stderr: "warn\n".into(),
            exit_code: 0,
        });
        assert_eq!(r.columns, vec!["field".to_string(), "value".to_string()]);
        assert_eq!(r.rows.len(), 3);
        assert_eq!(r.rows[0], vec!["exit_code".to_string(), "0".to_string()]);
        assert_eq!(r.rows[1], vec!["stdout".to_string(), "hi\n".to_string()]);
        assert_eq!(
            r.rows[2],
            vec!["stderr".to_string(), "warn\n".to_string()]
        );
    }

    #[tokio::test]
    async fn run_reports_missing_binary_clearly() {
        let r = BrowserRequest {
            bin: Some(PathBuf::from("/nonexistent/agent-browser-xyz")),
            ..req("open")
        };
        let err = BrowserExec::run(r).await.unwrap_err();
        match err {
            Error::Config(msg) => {
                assert!(msg.contains("not found"), "got: {msg}");
                assert!(msg.contains("agent-browser"), "got: {msg}");
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }
}
