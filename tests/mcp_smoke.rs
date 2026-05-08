//! End-to-end smoke test that runs the binary with no subcommand
//! (which boots the MCP server) and exchanges a minimal JSON-RPC
//! handshake over its stdio. Verifies that `mysql_exec` shows up in
//! `tools/list`.
//!
//! Transport framing: newline-delimited JSON (one JSON object per line),
//! confirmed from rmcp 1.6 `src/transport/async_rw.rs` which uses a
//! newline-scanning codec — NOT Content-Length framing.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tools-mcp"))
}

#[test]
fn test_mcp_lists_mysql_exec_tool() {
    let mut child = Command::new(binary_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn tools-mcp");

    let mut stdin = child.stdin.take().expect("no stdin");
    let stdout = child.stdout.take().expect("no stdout");
    let stderr = child.stderr.take().expect("no stderr");
    let mut reader = BufReader::new(stdout);

    // Framing: newline-delimited JSON (rmcp 1.6 stdio transport).
    // Protocol version "2024-11-05" is explicitly accepted per rmcp model.rs.
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke-test","version":"0.0.1"}}}"#;
    writeln!(stdin, "{initialize}").unwrap();

    let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    writeln!(stdin, "{initialized}").unwrap();

    let list_tools = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    writeln!(stdin, "{list_tools}").unwrap();
    stdin.flush().unwrap();

    let mut found_tool = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        if n == 0 {
            break;
        }
        if line.contains("\"id\":2") && line.contains("mysql_exec") {
            found_tool = true;
            break;
        }
    }

    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();

    if !found_tool {
        // Capture stderr for diagnosis.
        let mut err_buf = String::new();
        std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut err_buf).ok();
        eprintln!("---child stderr---\n{err_buf}\n---end---");
    }

    assert!(
        found_tool,
        "tools/list response did not contain mysql_exec within 10s"
    );
}

trait WaitTimeoutExt {
    fn wait_timeout(&mut self, dur: Duration) -> Option<std::process::ExitStatus>;
}

impl WaitTimeoutExt for std::process::Child {
    fn wait_timeout(&mut self, dur: Duration) -> Option<std::process::ExitStatus> {
        let deadline = std::time::Instant::now() + dur;
        while std::time::Instant::now() < deadline {
            match self.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return None,
            }
        }
        None
    }
}
