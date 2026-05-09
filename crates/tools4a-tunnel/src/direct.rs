use async_trait::async_trait;
use tools4a_core::{Result, Tunnel, TunnelEndpoint};

pub struct DirectTunnel {
    target_host: String,
    target_port: u16,
    active: bool,
}

impl DirectTunnel {
    pub fn new(target_host: String, target_port: u16) -> Self {
        Self {
            target_host,
            target_port,
            active: false,
        }
    }
}

#[async_trait]
impl Tunnel for DirectTunnel {
    async fn establish(&mut self) -> Result<TunnelEndpoint> {
        self.active = true;
        Ok(TunnelEndpoint {
            host: self.target_host.clone(),
            port: self.target_port,
        })
    }

    async fn close(&mut self) -> Result<()> {
        self.active = false;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_direct_tunnel() {
        let mut tunnel = DirectTunnel::new("localhost".to_string(), 3306);
        assert!(!tunnel.is_active());

        let endpoint = tunnel.establish().await.unwrap();
        assert_eq!(endpoint.host, "localhost");
        assert_eq!(endpoint.port, 3306);
        assert!(tunnel.is_active());

        tunnel.close().await.unwrap();
        assert!(!tunnel.is_active());
    }
}
