//! `tools4a mongo ...` dispatch.

use super::shared::{load_max_timeout_secs, print_warnings};
use crate::output::CliFormatter;
use tools4a_core::config::Config;
use tools4a_core::{Result, Service};
use tools4a_mongo::{MongoOrchestrator, MongoRequest};

pub(super) async fn execute(command: &str, config: Config, allow_write: bool) -> Result<()> {
    let tunnel = config.tunnel.clone();
    let max_timeout_secs = load_max_timeout_secs()?;
    let mut req = MongoRequest::from_config(config, command.to_string())?;
    req.allow_write = allow_write;
    req.max_timeout_secs = max_timeout_secs;
    let result = MongoOrchestrator::execute(req, tunnel).await?;
    print_warnings(&result);
    println!("{}", CliFormatter::format(&result));
    Ok(())
}
