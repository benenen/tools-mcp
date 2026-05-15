//! `tools4a browser ...` dispatch.

use super::shared::{cli_to_tunnel_config, print_warnings};
use crate::cli::Cli;
use crate::output::CliFormatter;
use tools4a_browser::{BrowserOrchestrator, BrowserRequest};
use tools4a_core::{Result, Service};

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    cli: &Cli,
    subcommand: String,
    args: Vec<String>,
    session: Option<String>,
    proxy: Option<String>,
    proxy_bypass: Option<String>,
    browser_args: Option<String>,
    bin: Option<std::path::PathBuf>,
    include_headers: bool,
) -> Result<()> {
    let req = BrowserRequest {
        subcommand,
        args,
        session,
        proxy,
        proxy_bypass,
        browser_args,
        bin,
    };

    let tunnel_config = cli_to_tunnel_config(cli)?;
    let result = BrowserOrchestrator::execute(req, tunnel_config).await?;
    print_warnings(&result);

    if include_headers {
        println!("{}", CliFormatter::format(&result));
        return Ok(());
    }

    super::stream_exec_rows(&result);
    Ok(())
}
