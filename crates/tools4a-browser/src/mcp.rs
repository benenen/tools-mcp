//! `browser_exec` MCP tool — params + `McpTool` impl. Same shape as
//! `tools4a_ssh::mcp`: params land directly in a typed Request +
//! TunnelConfig, then dispatch through the orchestrator.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tools4a_core::{
    ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

use crate::orchestrator::BrowserOrchestrator;
use crate::request::BrowserRequest;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BrowserExecParams {
    /// agent-browser subcommand (e.g. "open", "click", "snapshot",
    /// "batch", "eval", "cookies", "screenshot"). Passed through
    /// verbatim; tools4a does not enumerate or validate the set —
    /// adding new subcommands upstream needs no change here.
    pub subcommand: String,

    /// Positional + flag arguments after the subcommand. No shell
    /// interpretation; each entry becomes one argv element.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// agent-browser `--session <NAME>` — isolates daemon state. Use
    /// the same value across calls to share cookies / pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,

    /// agent-browser `--proxy <URL>`. Phase 1: pass this yourself if
    /// you need to route through SSH (e.g. socks5://127.0.0.1:1080
    /// after `ssh -D 1080 <bastion>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,

    /// agent-browser `--proxy-bypass <hosts>` (comma-separated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_bypass: Option<String>,

    /// agent-browser `--args <flags>` — extra Chromium launch args.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_args: Option<String>,

    /// Override the agent-browser binary path. Default lookup:
    /// $AGENT_BROWSER_BIN -> "agent-browser" on $PATH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,

    /// Tunnel kind. Phase 1: only "direct" (or omitted) works.
    /// "ssh" returns a config error with the Phase 2 deferral note.
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
}

pub struct BrowserMcp;

#[async_trait]
impl McpTool for BrowserMcp {
    const NAME: &'static str = "browser_exec";
    const DESCRIPTION: &'static str = "Run one `agent-browser` CLI subcommand (https://github.com/vercel-labs/agent-browser) \
         and return its captured stdout / stderr / exit code. The browser daemon persists \
         between calls, so a sequence of calls with the same `session` share cookies / pages. \
         The `agent-browser` binary must be installed separately on the host running tools4a.";
    type Params = BrowserExecParams;

    async fn invoke(params: BrowserExecParams) -> Result<ExecutionResult> {
        let req = BrowserRequest {
            subcommand: params.subcommand,
            args: params.args,
            session: params.session,
            proxy: params.proxy,
            proxy_bypass: params.proxy_bypass,
            browser_args: params.browser_args,
            bin: params.bin.map(std::path::PathBuf::from),
        };

        let tunnel = build_tunnel_config(
            params.tunnel,
            params.ssh_jump,
            params.ssh_user,
            params.ssh_password,
            params.ssh_key_path,
            params.ssh_port,
        )?;

        BrowserOrchestrator::execute(req, tunnel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invoke_rejects_proxy_conflict_with_ssh_tunnel() {
        // Phase 2: tunnel=ssh works — but if the user ALSO passes an
        // explicit proxy, that's a config conflict (tools4a injects
        // its own socks5:// proxy from the SOCKS tunnel endpoint).
        // The orchestrator returns Error::Config BEFORE attempting any
        // DNS / SSH connect, so this test is deterministic and offline.
        let params = BrowserExecParams {
            subcommand: "snapshot".into(),
            args: Vec::new(),
            session: None,
            proxy: Some("socks5://127.0.0.1:1080".into()),
            proxy_bypass: None,
            browser_args: None,
            bin: None,
            tunnel: Some(TunnelKind::Ssh),
            ssh_jump: Some(SshJumpInput::Single("bastion.example.com".into())),
            ssh_user: Some("admin".into()),
            ssh_password: None,
            ssh_key_path: None,
            ssh_port: None,
        };
        let err = BrowserMcp::invoke(params).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("conflict"), "got: {msg}");
        assert!(msg.contains("socks5"), "got: {msg}");
    }
}
