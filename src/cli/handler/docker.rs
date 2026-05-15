//! `tools4a docker ...` dispatch.

use super::shared::{cli_to_tunnel_config, print_warnings};
use crate::cli::{Cli, DockerCommand};
use crate::output::CliFormatter;
use std::collections::HashMap;
use tools4a_core::{Error, Result, Service};
use tools4a_docker::{DockerAction, DockerOrchestrator, DockerRequest};

pub(super) async fn execute(
    cli: &Cli,
    docker_host: Option<String>,
    unix_socket: Option<String>,
    action: DockerCommand,
) -> Result<()> {
    let tunnel_config = cli_to_tunnel_config(cli)?;
    let (action, allow_write) = match action {
        DockerCommand::Ps {
            all,
            limit,
            filters,
        } => {
            let filters_map = parse_kv_filters(&filters)?;
            (
                DockerAction::Ps {
                    all,
                    limit,
                    filters: filters_map,
                },
                false,
            )
        }
        DockerCommand::Inspect { container } => (DockerAction::Inspect { container }, false),
        DockerCommand::Logs {
            container,
            tail,
            stdout,
            stderr,
            timestamps,
            since,
        } => (
            DockerAction::Logs {
                container,
                tail: Some(tail),
                stdout,
                stderr,
                timestamps,
                since,
            },
            false,
        ),
        DockerCommand::Stats { container } => (DockerAction::Stats { container }, false),
        DockerCommand::Top { container, ps_args } => {
            (DockerAction::Top { container, ps_args }, false)
        }
        DockerCommand::Run {
            container,
            cmd,
            user,
            working_dir,
            env,
            privileged,
            allow_write,
        } => (
            DockerAction::Run {
                container,
                cmd,
                user,
                working_dir,
                env: if env.is_empty() { None } else { Some(env) },
                privileged,
            },
            allow_write,
        ),
        DockerCommand::Restart {
            container,
            timeout_secs,
            allow_write,
        } => (
            DockerAction::Restart {
                container,
                timeout_secs,
            },
            allow_write,
        ),
    };

    let req = DockerRequest {
        action,
        docker_host: docker_host.unwrap_or_else(|| "unix:///var/run/docker.sock".to_string()),
        unix_socket,
        allow_write,
        timeout_secs: cli.timeout,
    };
    let result = DockerOrchestrator::execute(req, tunnel_config).await?;
    print_warnings(&result);
    println!("{}", CliFormatter::format(&result));
    Ok(())
}

/// Convert repeated `--filter key=value` CLI args into the
/// `HashMap<String, Vec<String>>` shape Docker's filters take.
fn parse_kv_filters(raw: &[String]) -> Result<Option<HashMap<String, Vec<String>>>> {
    if raw.is_empty() {
        return Ok(None);
    }
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for kv in raw {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| Error::Config(format!("filter '{kv}' must be KEY=VALUE")))?;
        map.entry(k.to_string()).or_default().push(v.to_string());
    }
    Ok(Some(map))
}
