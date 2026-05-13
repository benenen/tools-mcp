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
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tools4a"))
}

#[test]
fn test_mcp_lists_mysql_exec_tool() {
    let mut child = Command::new(binary_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn tools4a");

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

    let mut found_mysql = false;
    let mut found_pgsql = false;
    let mut found_redis = false;
    let mut found_mongo = false;
    let mut found_http = false;
    let mut found_ssh = false;
    let mut found_browser = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        if n == 0 {
            break;
        }
        if line.contains("\"id\":2") {
            if line.contains("mysql_exec") {
                found_mysql = true;
            }
            if line.contains("pgsql_exec") {
                found_pgsql = true;
            }
            if line.contains("redis_exec") {
                found_redis = true;
            }
            if line.contains("mongo_exec") {
                found_mongo = true;
            }
            if line.contains("http_exec") {
                found_http = true;
            }
            if line.contains("ssh_exec") {
                found_ssh = true;
            }
            if line.contains("browser_exec") {
                found_browser = true;
            }
            break;
        }
    }

    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();

    if !found_mysql
        || !found_pgsql
        || !found_redis
        || !found_mongo
        || !found_http
        || !found_ssh
        || !found_browser
    {
        // Capture stderr for diagnosis.
        let mut err_buf = String::new();
        std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut err_buf).ok();
        eprintln!("---child stderr---\n{err_buf}\n---end---");
    }

    assert!(found_mysql, "tools/list missing mysql_exec");
    assert!(found_pgsql, "tools/list missing pgsql_exec");
    assert!(found_redis, "tools/list missing redis_exec");
    assert!(found_mongo, "tools/list missing mongo_exec");
    assert!(found_http, "tools/list missing http_exec");
    assert!(found_ssh, "tools/list missing ssh_exec");
    assert!(found_browser, "tools/list missing browser_exec");
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
