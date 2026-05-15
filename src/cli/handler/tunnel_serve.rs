//! `tools4a tunnel-serve ...` dispatch — long-running tunnel daemon.

use crate::cli::TunnelServeType;
use std::net::SocketAddr;
use std::path::PathBuf;
use tools4a_core::{Error, Result, SocksTunnel, SshTunnel, StreamLocalTunnel, Tunnel};

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    kind: TunnelServeType,
    listen: SocketAddr,
    ssh_jump: String,
    ssh_user: String,
    ssh_password: Option<String>,
    ssh_key_path: Option<PathBuf>,
    ssh_port: u16,
    target_host: Option<String>,
    target_port: Option<u16>,
    remote_socket: Option<String>,
) -> Result<()> {
    let jumps: Vec<String> = ssh_jump
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if jumps.is_empty() {
        return Err(Error::Config(
            "--ssh-jump must not be empty (single host or comma-separated chain)".to_string(),
        ));
    }

    // Validate per-type required + rejected fields.
    match kind {
        TunnelServeType::SshTcp => {
            if remote_socket.is_some() {
                return Err(Error::Config(
                    "--remote-socket is only valid with --type=ssh-streamlocal".to_string(),
                ));
            }
            if target_host.is_none() || target_port.is_none() {
                return Err(Error::Config(
                    "--type=ssh-tcp requires --target-host and --target-port".to_string(),
                ));
            }
        }
        TunnelServeType::SshStreamlocal => {
            if target_host.is_some() || target_port.is_some() {
                return Err(Error::Config(
                    "--target-host/--target-port are only valid with --type=ssh-tcp".to_string(),
                ));
            }
            if remote_socket.is_none() {
                return Err(Error::Config(
                    "--type=ssh-streamlocal requires --remote-socket".to_string(),
                ));
            }
        }
        TunnelServeType::SshSocks => {
            if target_host.is_some() || target_port.is_some() || remote_socket.is_some() {
                return Err(Error::Config(
                    "--type=ssh-socks doesn't take --target-host/--target-port/--remote-socket"
                        .to_string(),
                ));
            }
        }
    }

    // Build the right tunnel impl and establish.
    let mut tunnel: Box<dyn Tunnel> = match kind {
        TunnelServeType::SshTcp => {
            let t = SshTunnel::new(
                jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
                target_host.unwrap(),
                target_port.unwrap(),
            )?
            .with_listen_addr(listen);
            Box::new(t)
        }
        TunnelServeType::SshStreamlocal => {
            let t = StreamLocalTunnel::new(
                jumps,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
                remote_socket.unwrap(),
            )?
            .with_listen_addr(listen);
            Box::new(t)
        }
        TunnelServeType::SshSocks => {
            let t = SocksTunnel::new(jumps, ssh_user, ssh_password, ssh_key_path, ssh_port)?
                .with_listen_addr(listen);
            Box::new(t)
        }
    };

    let ep = tunnel.establish().await?;
    let shape = match kind {
        TunnelServeType::SshTcp => "ssh-tcp",
        TunnelServeType::SshStreamlocal => "ssh-streamlocal",
        TunnelServeType::SshSocks => "ssh-socks",
    };
    eprintln!(
        "tunnel-serve [{shape}] listening on {host}:{port} (Ctrl-C to stop)",
        host = ep.host,
        port = ep.port,
    );

    wait_for_shutdown_signal().await;

    eprintln!("tunnel-serve: shutting down");
    let _ = tunnel.close().await;
    Ok(())
}

/// Block until SIGINT or SIGTERM. On non-unix, falls back to ctrl-c only.
async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
