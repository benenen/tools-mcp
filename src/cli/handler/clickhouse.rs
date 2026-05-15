//! `tools4a clickhouse ...` dispatch.

use super::shared::{load_max_timeout_secs, print_warnings};
use crate::output::CliFormatter;
use tools4a_clickhouse::{ClickhouseOrchestrator, ClickhouseRequest};
use tools4a_core::config::Config;
use tools4a_core::{Result, Service};

pub(super) async fn execute(query: &str, config: Config, allow_write: bool) -> Result<()> {
    let tunnel = config.tunnel.clone();
    let max_timeout_secs = load_max_timeout_secs()?;
    let mut req = ClickhouseRequest::from_config(config, query.to_string())?;
    req.allow_write = allow_write;
    req.max_timeout_secs = max_timeout_secs;
    let result = ClickhouseOrchestrator::execute(req, tunnel).await?;
    print_warnings(&result);
    println!("{}", CliFormatter::format(&result));
    Ok(())
}
