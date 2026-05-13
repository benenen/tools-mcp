//! SOCKS5 wire-format encoder/decoder. Pure functions over bytes —
//! no IO, no allocations beyond the returned Vec for replies.
//!
//! Scope: SOCKS5 (RFC 1928) handshake + CONNECT request/reply.
//! UDP ASSOCIATE / BIND are out of scope. Only the no-auth method
//! (0x00) is supported; this is fine because the SOCKS listener is
//! bound to 127.0.0.1.

use crate::{Error, Result};

pub const SOCKS5_VER: u8 = 0x05;
pub const METHOD_NO_AUTH: u8 = 0x00;
pub const METHOD_NO_ACCEPTABLE: u8 = 0xFF;

pub const CMD_CONNECT: u8 = 0x01;

pub const ATYP_IPV4: u8 = 0x01;
pub const ATYP_DOMAIN: u8 = 0x03;
pub const ATYP_IPV6: u8 = 0x04;

/// SOCKS5 reply codes (RFC 1928 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReplyCode {
    Succeeded = 0x00,
    GeneralFailure = 0x01,
    #[allow(dead_code)]
    ConnectionNotAllowed = 0x02,
    #[allow(dead_code)]
    NetworkUnreachable = 0x03,
    HostUnreachable = 0x04,
    #[allow(dead_code)]
    ConnectionRefused = 0x05,
    #[allow(dead_code)]
    TtlExpired = 0x06,
    #[allow(dead_code)]
    CommandNotSupported = 0x07,
    AddressTypeNotSupported = 0x08,
}

/// Parsed CONNECT request from a SOCKS5 client.
#[derive(Debug, Clone)]
pub struct Request {
    /// Target host. For ATYP=Domain this is the raw bytes interpreted
    /// as UTF-8 (Chrome/agent-browser will send valid DNS names);
    /// for ATYP=IPv4/IPv6 this is the IP rendered as a string.
    pub host: String,
    pub port: u16,
    /// Raw ATYP byte echoed back in the reply.
    pub atyp: u8,
}

/// Parse the client greeting: `VER NMETHODS METHODS...`.
/// Returns Ok(()) if it advertises the no-auth method (the only one
/// we support); Err(Error::Service(...)) otherwise.
///
/// Caller-supplied buffer must contain at least the full greeting.
pub fn parse_greeting(buf: &[u8]) -> Result<()> {
    if buf.len() < 2 {
        return Err(Error::Service("SOCKS5 greeting too short".into()));
    }
    if buf[0] != SOCKS5_VER {
        return Err(Error::Service(format!(
            "SOCKS5: unsupported version 0x{:02x}",
            buf[0]
        )));
    }
    let n = buf[1] as usize;
    if buf.len() < 2 + n {
        return Err(Error::Service(
            "SOCKS5 greeting truncated (NMETHODS exceeds buffer)".into(),
        ));
    }
    if buf[2..2 + n].contains(&METHOD_NO_AUTH) {
        Ok(())
    } else {
        Err(Error::Service(
            "SOCKS5 client did not offer no-auth (0x00) method".into(),
        ))
    }
}

/// Encode the server's method-selection reply: `VER METHOD`.
/// On success returns the no-auth selection; on rejection returns
/// 0xFF and the caller is expected to close the connection.
pub fn write_greeting_reply(accepted: bool) -> [u8; 2] {
    [
        SOCKS5_VER,
        if accepted {
            METHOD_NO_AUTH
        } else {
            METHOD_NO_ACCEPTABLE
        },
    ]
}

/// Parse a SOCKS5 CONNECT request:
/// `VER CMD RSV ATYP DST.ADDR DST.PORT`.
///
/// Returns the parsed `Request` plus the number of bytes consumed.
pub fn parse_request(buf: &[u8]) -> Result<(Request, usize)> {
    if buf.len() < 4 {
        return Err(Error::Service("SOCKS5 request too short".into()));
    }
    if buf[0] != SOCKS5_VER {
        return Err(Error::Service(format!(
            "SOCKS5: unsupported version in request 0x{:02x}",
            buf[0]
        )));
    }
    if buf[1] != CMD_CONNECT {
        return Err(Error::Service(format!(
            "SOCKS5: unsupported command 0x{:02x} (only CONNECT)",
            buf[1]
        )));
    }
    // buf[2] is RSV — ignored.
    let atyp = buf[3];
    let (host, addr_len) = match atyp {
        ATYP_IPV4 => {
            if buf.len() < 4 + 4 + 2 {
                return Err(Error::Service("SOCKS5 IPv4 request truncated".into()));
            }
            let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
            (ip.to_string(), 4)
        }
        ATYP_DOMAIN => {
            if buf.len() < 5 {
                return Err(Error::Service(
                    "SOCKS5 Domain request truncated (no len)".into(),
                ));
            }
            let dlen = buf[4] as usize;
            if buf.len() < 5 + dlen + 2 {
                return Err(Error::Service("SOCKS5 Domain request truncated".into()));
            }
            let name = std::str::from_utf8(&buf[5..5 + dlen])
                .map_err(|_| Error::Service("SOCKS5 Domain not UTF-8".into()))?
                .to_string();
            (name, 1 + dlen) // leading length byte + name
        }
        ATYP_IPV6 => {
            if buf.len() < 4 + 16 + 2 {
                return Err(Error::Service("SOCKS5 IPv6 request truncated".into()));
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&buf[4..20]);
            (std::net::Ipv6Addr::from(octets).to_string(), 16)
        }
        other => {
            return Err(Error::Service(format!(
                "SOCKS5: unsupported ATYP 0x{other:02x}"
            )));
        }
    };
    let port_offset = 4 + addr_len;
    let port = u16::from_be_bytes([buf[port_offset], buf[port_offset + 1]]);
    let total = port_offset + 2;
    Ok((Request { host, port, atyp }, total))
}

/// Build the SOCKS5 reply:
/// `VER REP RSV ATYP BND.ADDR BND.PORT`.
///
/// BND.ADDR / BND.PORT echo back the client's requested ATYP/host:port.
/// Clients ignore these bytes for CONNECT but echoing makes tcpdump
/// debugging easier than the RFC-permitted 0.0.0.0:0.
pub fn write_request_reply(rep: ReplyCode, atyp_echo: u8, host: &str, port: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(22);
    out.push(SOCKS5_VER);
    out.push(rep as u8);
    out.push(0x00); // RSV
    match atyp_echo {
        ATYP_IPV4 => {
            out.push(ATYP_IPV4);
            let ip: std::net::Ipv4Addr = host.parse().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
            out.extend_from_slice(&ip.octets());
        }
        ATYP_DOMAIN => {
            out.push(ATYP_DOMAIN);
            let bytes = host.as_bytes();
            let len = bytes.len().min(255);
            out.push(len as u8);
            out.extend_from_slice(&bytes[..len]);
        }
        ATYP_IPV6 => {
            out.push(ATYP_IPV6);
            let ip: std::net::Ipv6Addr = host.parse().unwrap_or(std::net::Ipv6Addr::UNSPECIFIED);
            out.extend_from_slice(&ip.octets());
        }
        _ => {
            // Fallback: pretend it was IPv4 0.0.0.0.
            out.push(ATYP_IPV4);
            out.extend_from_slice(&[0, 0, 0, 0]);
        }
    }
    out.extend_from_slice(&port.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_no_auth_accepted() {
        parse_greeting(&[0x05, 0x01, 0x00]).unwrap();
    }

    #[test]
    fn greeting_no_auth_among_many() {
        parse_greeting(&[0x05, 0x03, 0x02, 0x00, 0x01]).unwrap();
    }

    #[test]
    fn greeting_rejects_when_no_auth_missing() {
        let err = parse_greeting(&[0x05, 0x02, 0x02, 0x01]).unwrap_err();
        match err {
            Error::Service(m) => assert!(m.contains("no-auth")),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn greeting_rejects_wrong_version() {
        let err = parse_greeting(&[0x04, 0x01, 0x00]).unwrap_err();
        match err {
            Error::Service(m) => assert!(m.contains("version")),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn greeting_rejects_truncated() {
        parse_greeting(&[0x05]).unwrap_err();
        parse_greeting(&[0x05, 0x02, 0x00]).unwrap_err();
    }

    #[test]
    fn greeting_reply_encodes_accept_and_reject() {
        assert_eq!(write_greeting_reply(true), [0x05, 0x00]);
        assert_eq!(write_greeting_reply(false), [0x05, 0xFF]);
    }

    #[test]
    fn request_parses_ipv4() {
        // VER=5 CMD=CONNECT RSV ATYP=IPv4 1.2.3.4 :80
        let buf = [0x05, 0x01, 0x00, 0x01, 1, 2, 3, 4, 0x00, 0x50];
        let (req, n) = parse_request(&buf).unwrap();
        assert_eq!(req.host, "1.2.3.4");
        assert_eq!(req.port, 80);
        assert_eq!(req.atyp, ATYP_IPV4);
        assert_eq!(n, 10);
    }

    #[test]
    fn request_parses_domain() {
        // VER=5 CMD=CONNECT RSV ATYP=Domain LEN="example.com" :443
        let mut buf = vec![0x05, 0x01, 0x00, 0x03, 11];
        buf.extend_from_slice(b"example.com");
        buf.extend_from_slice(&443u16.to_be_bytes());
        let (req, n) = parse_request(&buf).unwrap();
        assert_eq!(req.host, "example.com");
        assert_eq!(req.port, 443);
        assert_eq!(req.atyp, ATYP_DOMAIN);
        assert_eq!(n, 4 + 1 + 11 + 2);
    }

    #[test]
    fn request_parses_ipv6() {
        let mut buf = vec![0x05, 0x01, 0x00, 0x04];
        buf.extend_from_slice(&[0u8; 16]); // ::
        buf.extend_from_slice(&80u16.to_be_bytes());
        let (req, _) = parse_request(&buf).unwrap();
        assert_eq!(req.host, "::");
        assert_eq!(req.port, 80);
    }

    #[test]
    fn request_rejects_non_connect() {
        let buf = [0x05, 0x02, 0x00, 0x01, 0, 0, 0, 0, 0, 80];
        let err = parse_request(&buf).unwrap_err();
        match err {
            Error::Service(m) => assert!(m.contains("CONNECT")),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn request_rejects_truncated_domain() {
        let buf = [0x05, 0x01, 0x00, 0x03, 0x05, b'a']; // says len=5, has 1
        parse_request(&buf).unwrap_err();
    }

    #[test]
    fn request_rejects_unsupported_atyp() {
        let buf = [0x05, 0x01, 0x00, 0x99];
        parse_request(&buf).unwrap_err();
    }

    #[test]
    fn reply_succeeded_domain_echo() {
        let out = write_request_reply(ReplyCode::Succeeded, ATYP_DOMAIN, "example.com", 443);
        assert_eq!(out[0], 0x05);
        assert_eq!(out[1], 0x00);
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], ATYP_DOMAIN);
        assert_eq!(out[4], 11);
        assert_eq!(&out[5..16], b"example.com");
        assert_eq!(&out[16..18], &443u16.to_be_bytes());
    }

    #[test]
    fn reply_failure_ipv4_zero() {
        let out = write_request_reply(ReplyCode::HostUnreachable, ATYP_IPV4, "0.0.0.0", 0);
        assert_eq!(out, vec![0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
    }
}
