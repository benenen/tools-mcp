//! Tunnel runtime impls + a one-call helper that turns a `TunnelConfig`
//! into a ready-to-use `Box<dyn Tunnel>`. Lives in its own crate so each
//! leaf service crate can build a tunnel without needing the
//! orchestrator (which would create a dependency cycle).

mod direct;
mod ssh;

use tools4a_core::{Result, Tunnel, TunnelConfig};

pub use direct::DirectTunnel;
pub use ssh::SshTunnel;

/// Build the appropriate tunnel for a target `(host, port)` from a
/// `TunnelConfig`. `None` is treated as `Direct`.
pub fn build_tunnel(
    target_host: String,
    target_port: u16,
    tunnel_config: Option<TunnelConfig>,
) -> Result<Box<dyn Tunnel>> {
    match tunnel_config {
        None | Some(TunnelConfig::Direct) => {
            Ok(Box::new(DirectTunnel::new(target_host, target_port)))
        }
        Some(TunnelConfig::Ssh {
            ssh_jumps,
            ssh_user,
            ssh_password,
            ssh_key_path,
            ssh_port,
        }) => {
            let key_path = ssh_key_path.map(std::path::PathBuf::from);
            Ok(Box::new(SshTunnel::new(
                ssh_jumps,
                ssh_user,
                ssh_password,
                key_path,
                ssh_port,
                target_host,
                target_port,
            )?))
        }
    }
}
