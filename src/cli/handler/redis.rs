//! `tools4a redis ...` dispatch.

use super::shared::{load_max_timeout_secs, print_warnings};
use crate::output::CliFormatter;
use tools4a_core::config::Config;
use tools4a_core::{Result, Service};
use tools4a_redis::{RedisOrchestrator, RedisRequest};

pub(super) async fn execute(command: &str, config: Config) -> Result<()> {
    let tunnel = config.tunnel.clone();
    let max_timeout_secs = load_max_timeout_secs()?;
    let mut req = RedisRequest::from_config(config, command.to_string())?;
    req.max_timeout_secs = max_timeout_secs;
    let result = RedisOrchestrator::execute(req, tunnel).await?;
    print_warnings(&result);
    println!("{}", CliFormatter::format(&result));
    Ok(())
}
