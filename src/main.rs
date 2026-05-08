use clap::Parser;
use tools_mcp::cli::{Cli, CliHandler};
use tools_mcp::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.command.is_none() {
        println!("MCP mode not yet implemented. Use a subcommand (mysql) for CLI mode.");
        std::process::exit(1);
    }

    CliHandler::handle(cli).await
}
