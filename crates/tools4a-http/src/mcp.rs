//! `http_exec` MCP tool — params + `McpTool` impl. Unlike the
//! database tools, HTTP has no Profile/YAML support; params land
//! directly in `HttpRequestSpec` + `TunnelConfig`.

use crate::orchestrator::HttpOrchestrator;
use crate::request::{HttpAuth, HttpRequestSpec};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tools4a_core::config::ConfigLoader;
use tools4a_core::{
    Error, ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HttpExecParams {
    pub method: String,
    pub url: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub json: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basic: Option<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure: bool,

    /// Include HTML UI resource in the response. Disabled by default to
    /// save tokens (~1700 tokens per call). When enabled, returns an
    /// interactive HTTP response viewer alongside the JSON data.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_ui: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,

    /// Per-call execution timeout in seconds. Capped by the operator's
    /// `TOOLS4A_MAX_TIMEOUT_SECS` env var or TOML `[defaults]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

pub struct HttpMcp;

#[async_trait]
impl McpTool for HttpMcp {
    const NAME: &'static str = "http_exec";
    const DESCRIPTION: &'static str = "Send an HTTP/HTTPS request and return status, headers, and body. \
         Optionally route through an SSH jump host. \
         Same options as the `tools4a http` CLI subcommand.";
    type Params = HttpExecParams;

    async fn invoke(params: HttpExecParams) -> Result<ExecutionResult> {
        let max_timeout_secs =
            ConfigLoader::load_default_toml()?.and_then(|t| t.defaults.max_timeout_secs);
        let (mut req, tunnel) = params_to_request_and_tunnel(params)?;
        req.max_timeout_secs = max_timeout_secs;
        HttpOrchestrator::execute(req, tunnel).await
    }
}

fn params_to_request_and_tunnel(
    p: HttpExecParams,
) -> Result<(HttpRequestSpec, Option<tools4a_core::TunnelConfig>)> {
    let mut header_pairs: Vec<(String, String)> = Vec::new();
    for raw in &p.headers {
        let (name, value) = raw.split_once(':').ok_or_else(|| {
            Error::Config(format!(
                "header '{raw}' must be 'Name: Value' (missing ':')"
            ))
        })?;
        header_pairs.push((name.trim().to_string(), value.trim().to_string()));
    }
    if p.json {
        header_pairs.push(("Content-Type".to_string(), "application/json".to_string()));
    }

    let auth = match (p.bearer, p.basic) {
        (Some(token), None) => HttpAuth::Bearer(token),
        (None, Some(creds)) => {
            let (user, password) = creds
                .split_once(':')
                .ok_or_else(|| Error::Config("basic must be 'user:password'".to_string()))?;
            HttpAuth::Basic {
                user: user.to_string(),
                password: password.to_string(),
            }
        }
        (None, None) => HttpAuth::None,
        (Some(_), Some(_)) => {
            return Err(Error::Config(
                "bearer and basic are mutually exclusive".to_string(),
            ));
        }
    };

    let req = HttpRequestSpec {
        method: p.method,
        url: p.url,
        headers: header_pairs,
        body: p.data.map(|s| s.into_bytes()),
        auth,
        insecure: p.insecure,
        timeout_secs: p.timeout_secs,
        max_timeout_secs: None,
    };

    let tunnel_config = build_tunnel_config(
        p.tunnel,
        p.ssh_jump,
        p.ssh_user,
        p.ssh_password,
        p.ssh_key_path,
        p.ssh_port,
    )?;

    Ok((req, tunnel_config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_post() {
        let p = HttpExecParams {
            method: "POST".into(),
            url: "https://api.example.com/x".into(),
            headers: vec!["X-Foo: bar".into()],
            data: Some(r#"{"a":1}"#.into()),
            json: true,
            bearer: Some("tok".into()),
            basic: None,
            insecure: false,
            tunnel: None,
            ssh_jump: None,
            ssh_user: None,
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
            timeout_secs: None,
            include_ui: false,
        };
        let (req, tunnel) = params_to_request_and_tunnel(p).unwrap();
        assert_eq!(req.method, "POST");
        assert!(
            req.headers
                .contains(&("X-Foo".to_string(), "bar".to_string()))
        );
        assert!(
            req.headers
                .contains(&("Content-Type".to_string(), "application/json".to_string()))
        );
        assert!(matches!(req.auth, HttpAuth::Bearer(ref t) if t == "tok"));
        assert!(tunnel.is_none());
    }
}
