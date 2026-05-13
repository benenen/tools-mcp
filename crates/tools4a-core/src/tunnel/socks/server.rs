//! Per-connection SOCKS5 handler. Greedily reads the greeting +
//! request, dispatches the CONNECT through a `Connector`, then runs
//! `tokio::io::copy_bidirectional` until either half closes.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::tunnel::socks::codec::{
    ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6, ReplyCode, parse_greeting, parse_request,
    write_greeting_reply, write_request_reply,
};
use crate::tunnel::socks::connector::{Connector, Stream};
use crate::{Error, Result};

pub struct Socks5Server;

impl Socks5Server {
    /// Handle one accepted SOCKS5 client end-to-end.
    ///
    /// Reads the greeting + request from `inbound`, opens an outbound
    /// stream via `connector`, writes the appropriate reply back to
    /// `inbound`, and runs a bidirectional copy until either half EOFs.
    pub async fn serve_one<S: Stream>(mut inbound: S, connector: Arc<dyn Connector>) -> Result<()> {
        // --- Greeting ---------------------------------------------------
        // VER + NMETHODS + METHODS (up to 255). Read VER+NMETHODS first.
        let mut head = [0u8; 2];
        inbound
            .read_exact(&mut head)
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 greeting head read: {e}")))?;
        let nmethods = head[1] as usize;
        let mut methods = vec![0u8; nmethods];
        if nmethods > 0 {
            inbound
                .read_exact(&mut methods)
                .await
                .map_err(|e| Error::Service(format!("SOCKS5 greeting methods read: {e}")))?;
        }
        let mut greet = Vec::with_capacity(2 + nmethods);
        greet.extend_from_slice(&head);
        greet.extend_from_slice(&methods);
        let greet_ok = parse_greeting(&greet).is_ok();
        inbound
            .write_all(&write_greeting_reply(greet_ok))
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 greeting reply: {e}")))?;
        if !greet_ok {
            return Err(Error::Service("SOCKS5 client rejected at greeting".into()));
        }

        // --- Request ----------------------------------------------------
        // VER CMD RSV ATYP + (1 byte of address). Branch on ATYP for the rest.
        let mut req_head = [0u8; 5];
        inbound
            .read_exact(&mut req_head)
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 request header read: {e}")))?;
        let body_len = match req_head[3] {
            ATYP_IPV4 => 3 + 2,                      // 3 remaining IPv4 octets + port
            ATYP_DOMAIN => req_head[4] as usize + 2, // name body + port (len byte already in [4])
            ATYP_IPV6 => 15 + 2,                     // 15 remaining IPv6 octets + port
            other => {
                let reply = write_request_reply(
                    ReplyCode::AddressTypeNotSupported,
                    ATYP_IPV4,
                    "0.0.0.0",
                    0,
                );
                let _ = inbound.write_all(&reply).await;
                return Err(Error::Service(format!(
                    "SOCKS5: unsupported ATYP 0x{other:02x}"
                )));
            }
        };
        let mut req_buf = Vec::with_capacity(5 + body_len);
        req_buf.extend_from_slice(&req_head);
        req_buf.resize(5 + body_len, 0);
        inbound
            .read_exact(&mut req_buf[5..])
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 request body read: {e}")))?;
        let (req, _consumed) = parse_request(&req_buf)?;

        // --- Connect outbound -------------------------------------------
        let outbound = match connector.connect(&req.host, req.port).await {
            Ok(s) => s,
            Err(e) => {
                let reply =
                    write_request_reply(ReplyCode::HostUnreachable, req.atyp, &req.host, req.port);
                let _ = inbound.write_all(&reply).await;
                return Err(e);
            }
        };

        // --- Success reply ----------------------------------------------
        let reply = write_request_reply(ReplyCode::Succeeded, req.atyp, &req.host, req.port);
        inbound
            .write_all(&reply)
            .await
            .map_err(|e| Error::Service(format!("SOCKS5 success reply: {e}")))?;

        // --- Bidirectional copy -----------------------------------------
        let mut outbound = outbound;
        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::pin::Pin;

    /// In-memory Connector that records the requested (host, port) via
    /// a shared captured-bytes channel and returns a tokio duplex pipe.
    /// Anything the SOCKS server writes toward "the remote" lands in
    /// `captured`; anything in `canned_reply` is delivered from the
    /// mock back to the SOCKS server (which forwards it to its inbound).
    struct MockConnector {
        last_target: tokio::sync::Mutex<Option<(String, u16)>>,
        canned_reply: Vec<u8>,
        captured: Arc<tokio::sync::Mutex<Vec<u8>>>,
    }

    #[async_trait]
    impl Connector for MockConnector {
        async fn connect(&self, host: &str, port: u16) -> Result<Pin<Box<dyn Stream>>> {
            *self.last_target.lock().await = Some((host.to_string(), port));
            let (server_side, mut peer_side) = tokio::io::duplex(8192);
            let canned = self.canned_reply.clone();
            let captured = self.captured.clone();
            tokio::spawn(async move {
                let _ = peer_side.write_all(&canned).await;
                let mut buf = vec![0u8; 4096];
                while let Ok(n) = peer_side.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    captured.lock().await.extend_from_slice(&buf[..n]);
                }
            });
            Ok(Box::pin(server_side))
        }
    }

    #[tokio::test]
    async fn happy_path_domain_target() {
        let (mut client, server) = tokio::io::duplex(8192);

        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let mc = MockConnector {
            last_target: tokio::sync::Mutex::new(None),
            canned_reply: b"HTTP/1.0 200 OK\r\n\r\nhello".to_vec(),
            captured: captured.clone(),
        };
        // Verify the requested target via `captured` (which IS shared);
        // last_target is recorded inside the trait object but we don't
        // surface it through Connector, so the captured bytes are the
        // testable signal.
        let connector: Arc<dyn Connector> = Arc::new(mc);

        let task = tokio::spawn(async move { Socks5Server::serve_one(server, connector).await });

        // Client sends greeting (VER=5, NMETHODS=1, METHOD=no-auth)
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        // Read greeting reply
        let mut g = [0u8; 2];
        client.read_exact(&mut g).await.unwrap();
        assert_eq!(g, [0x05, 0x00]);
        // Send CONNECT example.com:80
        let mut req = vec![0x05, 0x01, 0x00, 0x03, 11];
        req.extend_from_slice(b"example.com");
        req.extend_from_slice(&80u16.to_be_bytes());
        client.write_all(&req).await.unwrap();
        // Read CONNECT reply
        let mut head = [0u8; 4];
        client.read_exact(&mut head).await.unwrap();
        assert_eq!(head, [0x05, 0x00, 0x00, 0x03]);
        let mut len = [0u8; 1];
        client.read_exact(&mut len).await.unwrap();
        let mut name = vec![0u8; len[0] as usize];
        client.read_exact(&mut name).await.unwrap();
        let mut port = [0u8; 2];
        client.read_exact(&mut port).await.unwrap();
        assert_eq!(name, b"example.com");
        assert_eq!(u16::from_be_bytes(port), 80);

        // Send payload toward "the remote".
        client.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
        // Read the canned response back.
        let mut resp = [0u8; 24];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(&resp, b"HTTP/1.0 200 OK\r\n\r\nhello");

        // Closing the client side ends the copy loop.
        drop(client);
        task.await.unwrap().unwrap();

        // Bytes the SOCKS server sent through to "the remote".
        assert_eq!(captured.lock().await.as_slice(), b"GET / HTTP/1.0\r\n\r\n");
    }

    /// Connector that always fails — used to verify HostUnreachable reply.
    struct FailingConnector;

    #[async_trait]
    impl Connector for FailingConnector {
        async fn connect(&self, _host: &str, _port: u16) -> Result<Pin<Box<dyn Stream>>> {
            Err(Error::Connection("simulated upstream failure".into()))
        }
    }

    #[tokio::test]
    async fn upstream_failure_surfaces_host_unreachable() {
        let (mut client, server) = tokio::io::duplex(8192);
        let connector: Arc<dyn Connector> = Arc::new(FailingConnector);
        let task = tokio::spawn(async move { Socks5Server::serve_one(server, connector).await });

        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut g = [0u8; 2];
        client.read_exact(&mut g).await.unwrap();

        let mut req = vec![0x05, 0x01, 0x00, 0x03, 11];
        req.extend_from_slice(b"example.com");
        req.extend_from_slice(&80u16.to_be_bytes());
        client.write_all(&req).await.unwrap();

        let mut head = [0u8; 4];
        client.read_exact(&mut head).await.unwrap();
        // REP=0x04 (HostUnreachable)
        assert_eq!(head, [0x05, 0x04, 0x00, 0x03]);

        let result = task.await.unwrap();
        assert!(matches!(result, Err(Error::Connection(_))));
    }

    /// Greeting without no-auth method => server replies 0x05 0xFF and errors.
    #[tokio::test]
    async fn greeting_without_no_auth_rejected() {
        let (mut client, server) = tokio::io::duplex(8192);
        let connector: Arc<dyn Connector> = Arc::new(FailingConnector);
        let task = tokio::spawn(async move { Socks5Server::serve_one(server, connector).await });

        // VER=5, NMETHODS=1, METHOD=0x02 (no no-auth offered)
        client.write_all(&[0x05, 0x01, 0x02]).await.unwrap();
        let mut g = [0u8; 2];
        client.read_exact(&mut g).await.unwrap();
        assert_eq!(g, [0x05, 0xFF]);

        let result = task.await.unwrap();
        assert!(matches!(result, Err(Error::Service(_))));
    }

    /// Non-CONNECT command should error.
    #[tokio::test]
    async fn non_connect_command_rejected() {
        let (mut client, server) = tokio::io::duplex(8192);
        let connector: Arc<dyn Connector> = Arc::new(FailingConnector);
        let task = tokio::spawn(async move { Socks5Server::serve_one(server, connector).await });

        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut g = [0u8; 2];
        client.read_exact(&mut g).await.unwrap();

        // VER=5 CMD=0x02 (BIND, unsupported) RSV ATYP=IPv4 1.2.3.4 :80
        let req = [0x05, 0x02, 0x00, 0x01, 1, 2, 3, 4, 0x00, 0x50];
        client.write_all(&req).await.unwrap();

        let result = task.await.unwrap();
        assert!(matches!(result, Err(Error::Service(ref m)) if m.contains("CONNECT")));
    }
}
