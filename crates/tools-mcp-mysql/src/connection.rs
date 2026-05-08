use mysql_async::{Conn, OptsBuilder, Pool};
use tools_mcp_core::{Connection, Error, Result, Tunnel};
use async_trait::async_trait;

pub struct MySQLConnection {
    tunnel: Box<dyn Tunnel>,
    user: String,
    password: Option<String>,
    database: Option<String>,
    pool: Option<Pool>,
    conn: Option<Conn>,
}

impl MySQLConnection {
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
            pool: None,
            conn: None,
        }
    }

    pub async fn get_conn(&mut self) -> Result<&mut Conn> {
        if self.conn.is_none() {
            self.connect().await?;
        }
        self.conn
            .as_mut()
            .ok_or_else(|| Error::Connection("Connection not established".to_string()))
    }
}

#[async_trait]
impl Connection for MySQLConnection {
    async fn connect(&mut self) -> Result<()> {
        let endpoint = self.tunnel.establish().await?;

        let mut builder = OptsBuilder::default()
            .ip_or_hostname(endpoint.host.clone())
            .tcp_port(endpoint.port)
            .user(Some(self.user.clone()));

        if let Some(ref password) = self.password {
            builder = builder.pass(Some(password.clone()));
        }

        if let Some(ref database) = self.database {
            builder = builder.db_name(Some(database.clone()));
        }

        let pool = Pool::new(builder);
        let conn = pool
            .get_conn()
            .await
            .map_err(|e: mysql_async::Error| Error::Service(format!("MySQL: {e}")))?;

        self.pool = Some(pool);
        self.conn = Some(conn);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
        if let Some(pool) = self.pool.take() {
            pool.disconnect()
                .await
                .map_err(|e: mysql_async::Error| Error::Service(format!("MySQL: {e}")))?;
        }
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
    use tools_mcp_core::TunnelEndpoint;

    /// Minimal Tunnel impl so this lib's tests don't depend on
    /// DirectTunnel (which lives in the bin crate).
    struct TestTunnel { active: bool }

    #[async_trait]
    impl Tunnel for TestTunnel {
        async fn establish(&mut self) -> Result<TunnelEndpoint> {
            self.active = true;
            Ok(TunnelEndpoint { host: "localhost".to_string(), port: 3306 })
        }
        async fn close(&mut self) -> Result<()> {
            self.active = false;
            Ok(())
        }
        fn is_active(&self) -> bool { self.active }
    }

    #[tokio::test]
    async fn test_mysql_connection_new() {
        let tunnel = Box::new(TestTunnel { active: false });
        let conn = MySQLConnection::new(
            tunnel,
            "root".to_string(),
            Some("password".to_string()),
            None,
        );
        assert!(!conn.is_connected());
    }
}
