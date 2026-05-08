use crate::connection::traits::Connection;
use crate::error::{Error, Result};
use crate::tunnel::Tunnel;
use async_trait::async_trait;
use mysql_async::{Conn, OptsBuilder, Pool};

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
        let conn = pool.get_conn().await?;

        self.pool = Some(pool);
        self.conn = Some(conn);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(conn) = self.conn.take() {
            drop(conn);
        }
        if let Some(pool) = self.pool.take() {
            pool.disconnect().await?;
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
    use crate::tunnel::DirectTunnel;

    #[tokio::test]
    async fn test_mysql_connection_new() {
        let tunnel = Box::new(DirectTunnel::new("localhost".to_string(), 3306));
        let conn = MySQLConnection::new(
            tunnel,
            "root".to_string(),
            Some("password".to_string()),
            None,
        );
        assert!(!conn.is_connected());
    }
}
