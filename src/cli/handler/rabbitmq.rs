//! `tools4a rabbitmq ...` dispatch.

use super::shared::{cli_to_tunnel_config, print_warnings};
use crate::cli::{Cli, RabbitmqCommand};
use crate::output::CliFormatter;
use tools4a_core::{Error, Result, Service};
use tools4a_rabbitmq::{
    RabbitmqAction, RabbitmqOrchestrator, RabbitmqRequest, orchestrator::default_port_for,
};

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    cli: &Cli,
    host: Option<String>,
    scheme: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    password: Option<String>,
    insecure: bool,
    action: RabbitmqCommand,
) -> Result<()> {
    let host = host.ok_or_else(|| Error::Config("rabbitmq --host is required".into()))?;
    let scheme = scheme.unwrap_or_else(|| "http".to_string());
    let port = port.unwrap_or_else(|| default_port_for(&scheme));
    let user = user.unwrap_or_else(|| "guest".to_string());
    let password = password.unwrap_or_else(|| "guest".to_string());

    let action = match action {
        RabbitmqCommand::ListQueues {
            vhost,
            name_pattern,
            limit,
        } => RabbitmqAction::ListQueues {
            vhost,
            name_pattern,
            limit,
        },
        RabbitmqCommand::QueueInfo { vhost, name } => RabbitmqAction::QueueInfo { vhost, name },
        RabbitmqCommand::GetMessages {
            vhost,
            queue,
            count,
            truncate_bytes,
        } => RabbitmqAction::GetMessages {
            vhost,
            queue,
            count,
            truncate_bytes,
        },
        RabbitmqCommand::ListBindings {
            vhost,
            source,
            destination,
        } => RabbitmqAction::ListBindings {
            vhost,
            source,
            destination,
        },
        RabbitmqCommand::Overview => RabbitmqAction::Overview,
    };

    let tunnel_config = cli_to_tunnel_config(cli)?;
    let req = RabbitmqRequest {
        action,
        scheme,
        host,
        port,
        user,
        password,
        insecure,
        timeout_secs: cli.timeout,
        max_timeout_secs: None,
    };
    let result = RabbitmqOrchestrator::execute(req, tunnel_config).await?;
    print_warnings(&result);
    println!("{}", CliFormatter::format(&result));
    Ok(())
}
