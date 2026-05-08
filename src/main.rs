use clap::Parser;
use tools_mcp::cli::{Cli, CliHandler};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.command.is_none() {
        eprintln!("MCP mode not yet implemented. Use a subcommand (mysql) for CLI mode.");
        std::process::exit(1);
    }

    if let Err(e) = CliHandler::handle(cli).await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
