//! Build a connected `bollard::Docker` from a target spec. Two shapes:
//! - `UnixSocket(path)` — local unix domain socket.
//! - `Tcp(addr)` — direct TCP, or tunneled TCP where the caller has
//!   already established a tunnel and substituted `127.0.0.1:<random>`
//!   as the addr.
//!
//! Tunneling itself lives in `orchestrator.rs` — by the time we get here,
//! any tunnel has already been established and its local endpoint is
//! folded into `ConnectTarget::Tcp`.

use bollard::{API_DEFAULT_VERSION, Docker};
use tools4a_core::{Error, Result};

/// What kind of Docker daemon endpoint to connect to.
#[derive(Debug, Clone)]
pub enum ConnectTarget {
    /// Local unix domain socket. Accepts `unix:///path` or bare `/path`.
    UnixSocket(String),
    /// TCP address, e.g. `host:port` or `tcp://host:port` or
    /// `127.0.0.1:<random>` (the tunneled form).
    Tcp(String),
}

/// Parse a docker_host string into a `ConnectTarget`. Recognizes the
/// three common scheme prefixes Docker CLI uses.
pub fn parse_docker_host(docker_host: &str) -> Result<ConnectTarget> {
    let trimmed = docker_host.trim();
    if trimmed.is_empty() {
        return Err(Error::Config(
            "docker_host must not be empty (try unix:///var/run/docker.sock or tcp://host:2375)"
                .to_string(),
        ));
    }
    if let Some(path) = trimmed.strip_prefix("unix://") {
        return Ok(ConnectTarget::UnixSocket(path.to_string()));
    }
    if trimmed.starts_with('/') {
        return Ok(ConnectTarget::UnixSocket(trimmed.to_string()));
    }
    let stripped = trimmed
        .strip_prefix("tcp://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    Ok(ConnectTarget::Tcp(stripped.to_string()))
}

/// Connect to a Docker daemon. Timeout is in seconds.
pub fn connect_docker(target: &ConnectTarget, timeout_secs: u64) -> Result<Docker> {
    match target {
        ConnectTarget::UnixSocket(path) => {
            Docker::connect_with_unix(path, timeout_secs, API_DEFAULT_VERSION).map_err(|e| {
                Error::Connection(format!("docker unix connect to {path} failed: {e}"))
            })
        }
        ConnectTarget::Tcp(addr) => {
            Docker::connect_with_http(addr, timeout_secs, API_DEFAULT_VERSION).map_err(|e| {
                Error::Connection(format!("docker http connect to {addr} failed: {e}"))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unix_scheme() {
        let t = parse_docker_host("unix:///var/run/docker.sock").unwrap();
        assert!(matches!(t, ConnectTarget::UnixSocket(ref p) if p == "/var/run/docker.sock"));
    }

    #[test]
    fn parse_bare_path_is_unix() {
        let t = parse_docker_host("/var/run/docker.sock").unwrap();
        assert!(matches!(t, ConnectTarget::UnixSocket(ref p) if p == "/var/run/docker.sock"));
    }

    #[test]
    fn parse_tcp_scheme() {
        let t = parse_docker_host("tcp://docker.local:2375").unwrap();
        assert!(matches!(t, ConnectTarget::Tcp(ref a) if a == "docker.local:2375"));
    }

    #[test]
    fn parse_bare_host_port_is_tcp() {
        let t = parse_docker_host("127.0.0.1:2375").unwrap();
        assert!(matches!(t, ConnectTarget::Tcp(ref a) if a == "127.0.0.1:2375"));
    }

    #[test]
    fn parse_http_scheme_is_tcp() {
        let t = parse_docker_host("http://docker.local:2375").unwrap();
        assert!(matches!(t, ConnectTarget::Tcp(ref a) if a == "docker.local:2375"));
    }

    #[test]
    fn parse_empty_errors() {
        assert!(matches!(parse_docker_host(""), Err(Error::Config(_))));
        assert!(matches!(parse_docker_host("   "), Err(Error::Config(_))));
    }
}
