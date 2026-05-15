//! `tools4a milvus ...` dispatch.

use super::shared::{cli_to_tunnel_config, print_warnings};
use crate::cli::{Cli, MilvusCommand};
use crate::output::CliFormatter;
use tools4a_core::{Error, Result, Service};
use tools4a_milvus::{MilvusAction, MilvusOrchestrator, MilvusRequest};

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    cli: &Cli,
    host: Option<String>,
    scheme: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    password: Option<String>,
    action: MilvusCommand,
) -> Result<()> {
    let host = host.ok_or_else(|| Error::Config("milvus --host is required".into()))?;
    let scheme = scheme.unwrap_or_else(|| "http".to_string());
    let port = port.unwrap_or(19530);

    let (action, allow_write) = match action {
        MilvusCommand::ListDatabases => (MilvusAction::ListDatabases, false),
        MilvusCommand::ListCollections => (MilvusAction::ListCollections, false),
        MilvusCommand::DescribeCollection { name } => {
            (MilvusAction::DescribeCollection { name }, false)
        }
        MilvusCommand::CollectionStats { name } => (MilvusAction::CollectionStats { name }, false),
        MilvusCommand::ListPartitions { collection } => {
            (MilvusAction::ListPartitions { collection }, false)
        }
        MilvusCommand::Query {
            collection,
            expr,
            output_fields,
            partition_names,
            limit,
            include_vectors,
        } => (
            MilvusAction::Query {
                collection,
                expr,
                output_fields,
                partition_names,
                limit,
                include_vectors,
            },
            false,
        ),
        MilvusCommand::Search {
            collection,
            vectors,
            metric,
            limit,
            output_fields,
            filter,
            anns_field,
            include_vectors,
        } => {
            let parsed: Vec<Vec<f32>> = serde_json::from_str(&vectors).map_err(|e| {
                Error::Config(format!(
                    "--vectors must be a JSON 2D float array, e.g. '[[0.1,0.2,...]]': {e}"
                ))
            })?;
            (
                MilvusAction::Search {
                    collection,
                    vectors: parsed,
                    metric,
                    limit,
                    output_fields,
                    filter,
                    anns_field,
                    include_vectors,
                },
                false,
            )
        }
        MilvusCommand::DropCollection { name, allow_write } => {
            (MilvusAction::DropCollection { name }, allow_write)
        }
        MilvusCommand::LoadCollection { name, allow_write } => {
            (MilvusAction::LoadCollection { name }, allow_write)
        }
        MilvusCommand::ReleaseCollection { name, allow_write } => {
            (MilvusAction::ReleaseCollection { name }, allow_write)
        }
    };

    let tunnel_config = cli_to_tunnel_config(cli)?;
    let req = MilvusRequest {
        action,
        scheme,
        host,
        port,
        username: user,
        password,
        allow_write,
        timeout_secs: cli.timeout,
        max_timeout_secs: None,
    };
    let result = MilvusOrchestrator::execute(req, tunnel_config).await?;
    print_warnings(&result);
    println!("{}", CliFormatter::format(&result));
    Ok(())
}
