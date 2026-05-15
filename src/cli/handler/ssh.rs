//! `tools4a ssh ...` dispatch.

use super::shared::{cli_to_tunnel_config, load_max_timeout_secs, print_warnings};
use crate::cli::Cli;
use crate::output::CliFormatter;
use tools4a_core::{Result, Service};
use tools4a_ssh::{SshDirectOrchestrator, SshExecRequest};

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    cli: &Cli,
    command: String,
    host: String,
    port: u16,
    user: String,
    password: Option<String>,
    key_path: Option<std::path::PathBuf>,
    include_headers: bool,
) -> Result<()> {
    let max_timeout_secs = load_max_timeout_secs()?;
    let req = SshExecRequest {
        host,
        port,
        user,
        password,
        key_path,
        command,
        timeout_secs: cli.timeout,
        max_timeout_secs,
    };

    let tunnel_config = cli_to_tunnel_config(cli)?;
    let result = SshDirectOrchestrator::execute(req, tunnel_config).await?;
    print_warnings(&result);

    if include_headers {
        println!("{}", CliFormatter::format(&result));
        return Ok(());
    }

    // Default: stream stdout to stdout, stderr to stderr, exit with the
    // remote exit code.
    super::stream_exec_rows(&result);
    Ok(())
}
