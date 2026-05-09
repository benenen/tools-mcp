use async_trait::async_trait;
use redis::Client;
use redis::aio::MultiplexedConnection;
use tools4a_core::{Connection, Error, Result, Tunnel};

pub struct RedisConnection {
    tunnel: Box<dyn Tunnel>,
    password: Option<String>,
    db: u32,
    client: Option<Client>,
    conn: Option<MultiplexedConnection>,
}

impl RedisConnection {
    pub fn new(tunnel: Box<dyn Tunnel>, password: Option<String>, db: u32) -> Self {
        Self {
            tunnel,
            password,
            db,
            client: None,
            conn: None,
        }
    }

    pub async fn get_conn(&mut self) -> Result<&mut MultiplexedConnection> {
        if self.conn.is_none() {
            self.connect().await?;
        }
        self.conn
            .as_mut()
            .ok_or_else(|| Error::Connection("Redis connection not established".to_string()))
    }
}

#[async_trait]
impl Connection for RedisConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;
        // redis::Client::open accepts a URL string. Build a redis://[:pwd@]host:port/db URL.
        let auth = match &self.password {
            Some(pwd) => format!(":{}@", urlencoding::encode(pwd)),
            None => String::new(),
        };
        let url = format!(
            "redis://{auth}{host}:{port}/{db}",
            host = endpoint.host,
            port = endpoint.port,
            db = self.db,
        );
        let client = Client::open(url)
            .map_err(|e: redis::RedisError| Error::Service(format!("Redis: {e}")))?;
        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e: redis::RedisError| Error::Service(format!("Redis: {e}")))?;
        self.client = Some(client);
        self.conn = Some(conn);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // MultiplexedConnection has no explicit close; dropping it terminates the multiplex.
        self.conn = None;
        self.client = None;
        self.tunnel.close().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.conn.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools4a_core::TunnelEndpoint;

    /// Minimal Tunnel impl so this lib's tests don't depend on
    /// DirectTunnel (which lives in the bin crate).
    struct TestTunnel {
        active: bool,
    }

    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint {
                host: "localhost".to_string(),
                port: 6379,
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

    #[tokio::test]
    async fn test_redis_connection_new() {
        let tunnel = Box::new(TestTunnel { active: false });
        let conn = RedisConnection::new(tunnel, Some("password".to_string()), 0);
        assert!(!conn.is_connected());
    }
}
