use clap::Parser;
use tools_mcp::cli::{Cli, CliHandler};
use tools_mcp::mcp;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = if cli.command.is_none() {
        // No subcommand -> run MCP server over stdio.
        mcp::serve_stdio().await
    } else {
        CliHandler::handle(cli).await
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
