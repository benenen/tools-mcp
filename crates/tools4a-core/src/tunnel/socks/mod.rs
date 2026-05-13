//! SOCKS5 server building blocks. Used by `SocksTunnel` to listen
//! locally and forward each accepted connection through an SSH
//! session as a `direct-tcpip` channel. The codec sub-module is
//! pure (bytes in -> bytes out, no IO).

pub mod codec;

pub use codec::{
    ReplyCode, Request, parse_greeting, parse_request, write_greeting_reply, write_request_reply,
};
