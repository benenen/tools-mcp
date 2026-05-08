//! HTTP request input shape — independent of any caller (CLI, MCP, future tests).

/// Authentication scheme to apply to the outgoing request.
#[derive(Debug, Clone)]
pub enum HttpAuth {
    None,
    /// `Authorization: Bearer <token>`.
    Bearer(String),
    /// `Authorization: Basic <base64(user:pass)>`.
    Basic { user: String, password: String },
}

/// Resolved HTTP request to execute. Caller (CLI handler / MCP tool) builds
/// this from the user's flags / JSON params; the lib doesn't care where the
/// fields came from.
#[derive(Debug, Clone)]
pub struct HttpRequestSpec {
    /// Method name (uppercased internally before sending).
    pub method: String,
    /// Full request URL including scheme + host + path + query.
    pub url: String,
    /// Extra headers as (name, value) pairs. Stored in insertion order.
    pub headers: Vec<(String, String)>,
    /// Optional request body. Already-encoded bytes; the lib doesn't transform it.
    pub body: Option<Vec<u8>>,
    /// Authentication scheme.
    pub auth: HttpAuth,
    /// If true, accept invalid TLS certs (e.g. self-signed). Default: false.
    pub insecure: bool,
}
