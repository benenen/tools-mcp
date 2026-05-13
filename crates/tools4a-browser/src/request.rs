//! Browser request input shape — independent of any caller (CLI, MCP).

/// One agent-browser CLI invocation.
///
/// Modeled after `SshExecRequest`: the caller fully specifies the
/// command line; tools4a doesn't parse or rewrite agent-browser
/// subcommands. agent-browser owns the subcommand surface — adding a
/// new one upstream needs no change here.
#[derive(Debug, Clone)]
pub struct BrowserRequest {
    /// Subcommand to invoke (e.g. `open`, `click`, `snapshot`, `batch`,
    /// `eval`, `cookies`, `screenshot`). Passed through verbatim as the
    /// first positional argument to `agent-browser`.
    pub subcommand: String,

    /// Positional + flag arguments that follow `<subcommand>`. Passed
    /// directly to `Command::args` — no shell interpretation.
    pub args: Vec<String>,

    /// Optional `--session <NAME>` to isolate daemon state. None = use
    /// agent-browser's default session.
    pub session: Option<String>,

    /// Optional `--proxy <URL>` (e.g. `socks5://127.0.0.1:1080` for
    /// users who set up their own SSH SOCKS forward via `ssh -D`).
    pub proxy: Option<String>,

    /// Optional `--proxy-bypass <hosts>` (comma-separated).
    pub proxy_bypass: Option<String>,

    /// Optional `--args <flags>` — extra Chromium launch arguments.
    pub browser_args: Option<String>,

    /// Path to the agent-browser binary. If None, the runner looks up
    /// `$AGENT_BROWSER_BIN`, then falls back to `agent-browser` on `$PATH`.
    pub bin: Option<std::path::PathBuf>,
}
