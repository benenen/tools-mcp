//! `CliHandler::handle` — top-level CLI command dispatch. Each per-leaf
//! submodule owns the typed-Request construction and `Service::execute`
//! call for one subcommand.

mod browser;
mod clickhouse;
mod docker;
mod http;
mod milvus;
mod mongo;
mod mysql;
mod pgsql;
mod rabbitmq;
mod redis;
mod shared;
mod ssh;
mod tunnel_serve;

use crate::cli::{Cli, Commands};
use shared::build_config;
use tools4a_core::config::ServiceType;
use tools4a_core::{Error, ExecutionResult, Result};

pub struct CliHandler;

impl CliHandler {
    pub async fn handle(cli: Cli) -> Result<()> {
        match cli.command.clone() {
            Some(Commands::Mysql {
                query,
                host,
                port,
                user,
                password,
                database,
                profile,
                allow_write,
            }) => {
                let config = build_config(
                    &cli,
                    ServiceType::Mysql,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None,
                    profile,
                )?;
                mysql::execute(&query, config, allow_write).await
            }
            Some(Commands::Pgsql {
                query,
                host,
                port,
                user,
                password,
                database,
                profile,
                allow_write,
            }) => {
                let config = build_config(
                    &cli,
                    ServiceType::Pgsql,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None,
                    profile,
                )?;
                pgsql::execute(&query, config, allow_write).await
            }
            Some(Commands::Clickhouse {
                query,
                host,
                port,
                user,
                password,
                database,
                profile,
                allow_write,
            }) => {
                let config = build_config(
                    &cli,
                    ServiceType::Clickhouse,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None,
                    profile,
                )?;
                clickhouse::execute(&query, config, allow_write).await
            }
            Some(Commands::Redis {
                command,
                host,
                port,
                password,
                db,
                profile,
            }) => {
                let config = shared::build_config_redis(&cli, host, port, password, db, profile)?;
                redis::execute(&command, config).await
            }
            Some(Commands::Mongo {
                command,
                host,
                port,
                user,
                password,
                database,
                profile,
                allow_write,
            }) => {
                let config = build_config(
                    &cli,
                    ServiceType::Mongo,
                    host,
                    port,
                    user,
                    password,
                    database,
                    None,
                    profile,
                )?;
                mongo::execute(&command, config, allow_write).await
            }
            Some(Commands::Http {
                method,
                url,
                headers,
                data,
                data_file,
                json,
                bearer,
                basic,
                insecure,
                include_headers,
            }) => {
                http::execute(
                    &cli,
                    method,
                    url,
                    headers,
                    data,
                    data_file,
                    json,
                    bearer,
                    basic,
                    insecure,
                    include_headers,
                )
                .await
            }
            Some(Commands::Ssh {
                command,
                host,
                port,
                user,
                password,
                key_path,
                include_headers,
            }) => {
                ssh::execute(
                    &cli,
                    command,
                    host,
                    port,
                    user,
                    password,
                    key_path,
                    include_headers,
                )
                .await
            }
            Some(Commands::Browser {
                subcommand,
                args,
                session,
                proxy,
                proxy_bypass,
                browser_args,
                bin,
                include_headers,
            }) => {
                browser::execute(
                    &cli,
                    subcommand,
                    args,
                    session,
                    proxy,
                    proxy_bypass,
                    browser_args,
                    bin,
                    include_headers,
                )
                .await
            }
            Some(Commands::Docker {
                docker_host,
                unix_socket,
                action,
            }) => docker::execute(&cli, docker_host, unix_socket, action).await,
            Some(Commands::Milvus {
                host,
                scheme,
                port,
                user,
                password,
                action,
            }) => milvus::execute(&cli, host, scheme, port, user, password, action).await,
            Some(Commands::Rabbitmq {
                host,
                scheme,
                port,
                user,
                password,
                insecure,
                action,
            }) => {
                rabbitmq::execute(&cli, host, scheme, port, user, password, insecure, action).await
            }
            Some(Commands::TunnelServe {
                kind,
                listen,
                ssh_jump,
                ssh_user,
                ssh_password,
                ssh_key_path,
                ssh_port,
                target_host,
                target_port,
                remote_socket,
            }) => {
                tunnel_serve::execute(
                    kind,
                    listen,
                    ssh_jump,
                    ssh_user,
                    ssh_password,
                    ssh_key_path,
                    ssh_port,
                    target_host,
                    target_port,
                    remote_socket,
                )
                .await
            }
            None => Err(Error::Config(
                "No command specified. Run with --help for usage.".to_string(),
            )),
        }
    }
}

/// Shared post-orchestrator output path for tools that return the
/// 3-row layout `[exit_code | stdout | stderr]` (ssh-direct + browser):
/// stream stdout to stdout, stderr to stderr, and `process::exit` with
/// the captured exit code on non-zero so the parent shell sees it.
pub(super) fn stream_exec_rows(result: &ExecutionResult) {
    let mut exit_code: i32 = 0;
    for row in &result.rows {
        if row.len() < 2 {
            continue;
        }
        match row[0].as_str() {
            "exit_code" => {
                exit_code = row[1].parse().unwrap_or(0);
            }
            "stdout" => {
                use std::io::Write;
                let _ = std::io::stdout().write_all(row[1].as_bytes());
            }
            "stderr" => {
                use std::io::Write;
                let _ = std::io::stderr().write_all(row[1].as_bytes());
            }
            _ => {}
        }
    }
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_handler_new() {
        let _handler = CliHandler;
    }
}
