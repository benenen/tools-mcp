use async_trait::async_trait;
use mongodb::{Client, options::ClientOptions};
use tools_mcp_core::{Connection, Error, Result, Tunnel};

pub struct MongoConnection {
    tunnel: Box<dyn Tunnel>,
    user: Option<String>,
    password: Option<String>,
    database: String,
    client: Option<Client>,
}

impl MongoConnection {
    pub fn new(
        tunnel: Box<dyn Tunnel>,
        user: Option<String>,
        password: Option<String>,
        database: String,
    ) -> Self {
        Self {
            tunnel,
            user,
            password,
            database,
            client: None,
        }
    }

    pub fn client(&self) -> Result<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| Error::Connection("Mongo connection not established".to_string()))
    }

    pub fn database_name(&self) -> &str {
        &self.database
    }
}

#[async_trait]
impl Connection for MongoConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;

        // Build URI: mongodb://[user:pass@]host:port
        let auth = match (&self.user, &self.password) {
            (Some(u), Some(p)) => format!("{}:{}@", urlencoding::encode(u), urlencoding::encode(p)),
            (Some(u), None) => format!("{}@", urlencoding::encode(u)),
            _ => String::new(),
        };
        let uri = format!("mongodb://{auth}{}:{}", endpoint.host, endpoint.port);

        let opts = ClientOptions::parse(&uri)
            .await
            .map_err(|e| Error::Service(format!("Mongo: {e}")))?;
        let client =
            Client::with_options(opts).map_err(|e| Error::Service(format!("Mongo: {e}")))?;

        self.client = Some(client);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            drop(client);
        }
        self.tunnel.close().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.client.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools_mcp_core::TunnelEndpoint;

    struct TestTunnel {
        active: bool,
    }
    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint {
                host: "localhost".to_string(),
                port: 27017,
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
    async fn test_mongo_connection_new() {
        let t = Box::new(TestTunnel { active: false });
        let c = MongoConnection::new(t, Some("u".into()), Some("p".into()), "test".into());
        assert!(!c.is_connected());
    }
}
