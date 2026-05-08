use crate::error::Result;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct TunnelEndpoint {
    pub host: String,
    pub port: u16,
}

#[async_trait]
pub trait Tunnel: Send + Sync {
    async fn establish(&mut self) -> Result<TunnelEndpoint>;
    async fn close(&mut self) -> Result<()>;
    fn is_active(&self) -> bool;
}
