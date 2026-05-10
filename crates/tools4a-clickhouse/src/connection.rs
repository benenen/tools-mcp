use async_trait::async_trait;
use clickhouse::Client;
use tools4a_core::{Connection, Error, Result, Tunnel};

pub struct ClickhouseConnection {
    tunnel: Box<dyn Tunnel>,
    user: String,
    password: Option<String>,
    database: Option<String>,
    read_only: bool,
    client: Option<Client>,
}

impl ClickhouseConnection {
    pub fn new(
        tunnel: Box<dyn Tunnel>,
        user: String,
        password: Option<String>,
        database: Option<String>,
        read_only: bool,
    ) -> Self {
        Self {
            tunnel,
            user,
            password,
            database,
            read_only,
            client: None,
        }
    }

    pub fn client(&self) -> Result<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| Error::Connection("Clickhouse connection not established".to_string()))
    }
}

#[async_trait]
impl Connection for ClickhouseConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;
        // v1 = HTTP only. HTTPS-via-tunnel needs a custom HttpClient impl
        // (the official `clickhouse` crate doesn't expose reqwest's
        // `resolve(host, addr)` API for SNI preservation). Out of scope.
        let url = format!("http://{}:{}", endpoint.host, endpoint.port);
        let mut client = Client::default().with_url(url).with_user(self.user.clone());
        if let Some(ref pw) = self.password {
            client = client.with_password(pw.clone());
        }
        if let Some(ref db) = self.database {
            client = client.with_database(db.clone());
        }
        if self.read_only {
            // Belt-and-suspenders alongside the orchestrator-level
            // `is_readonly_sql` whitelist. ClickHouse's `readonly=1`
            // session setting rejects writes server-side.
            client = client.with_setting("readonly", "1");
        }
        self.client = Some(client);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.client = None;
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
                port: 8123,
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
    async fn test_clickhouse_connection_new() {
        let t = Box::new(TestTunnel { active: false });
        let c = ClickhouseConnection::new(t, "default".to_string(), None, None, true);
        assert!(!c.is_connected());
    }
}
