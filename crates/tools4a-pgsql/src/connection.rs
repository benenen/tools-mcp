use async_trait::async_trait;
use tokio_postgres::{Client, Config, NoTls};
use tools4a_core::{Connection, Error, Result, Tunnel};

pub struct PgsqlConnection {
    tunnel: Box<dyn Tunnel>,
    user: String,
    password: Option<String>,
    database: Option<String>,
    client: Option<Client>,
}

impl PgsqlConnection {
    pub fn new(
        tunnel: Box<dyn Tunnel>,
        user: String,
        password: Option<String>,
        database: Option<String>,
    ) -> Self {
        Self {
            tunnel,
            user,
            password,
            database,
            client: None,
        }
    }

    pub fn client(&mut self) -> Result<&mut Client> {
        self.client
            .as_mut()
            .ok_or_else(|| Error::Connection("Pgsql connection not established".to_string()))
    }
}

#[async_trait]
impl Connection for PgsqlConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;

        let mut cfg = Config::new();
        cfg.host(&endpoint.host)
            .port(endpoint.port)
            .user(&self.user);
        if let Some(ref pw) = self.password {
            cfg.password(pw);
        }
        if let Some(ref db) = self.database {
            cfg.dbname(db);
        }

        let (client, connection) = cfg
            .connect(NoTls)
            .await
            .map_err(|e| Error::Service(format!("Pgsql: {e}")))?;

        // Background driver task — drops when client is dropped.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("pgsql connection task error: {e}");
            }
        });

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
    use tools4a_core::TunnelEndpoint;

    struct TestTunnel {
        active: bool,
    }

    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint {
                host: "localhost".to_string(),
                port: 5432,
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
    async fn test_pgsql_connection_new() {
        let t = Box::new(TestTunnel { active: false });
        let c = PgsqlConnection::new(t, "u".to_string(), Some("p".to_string()), None);
        assert!(!c.is_connected());
    }
}
