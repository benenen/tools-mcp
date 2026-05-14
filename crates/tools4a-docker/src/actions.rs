//! Per-action functions: each one takes a connected `bollard::Docker` +
//! typed arguments and returns an `ExecutionResult` shaped for the CLI
//! table renderer. Read-only actions (ps/inspect/logs/stats/top) never
//! mutate; the orchestrator gates write actions (run/restart) with
//! `allow_write`.

use std::collections::HashMap;

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::query_parameters::{
    ListContainersOptionsBuilder, LogsOptionsBuilder, RestartContainerOptionsBuilder,
    StatsOptionsBuilder, TopOptionsBuilder,
};
use futures_util::stream::StreamExt;
use tools4a_core::{Error, ExecutionResult, Result};

/// One of the seven supported actions. The orchestrator constructs this
/// from its typed Request; this module dispatches on it in `run::run`.
#[derive(Debug, Clone)]
pub enum DockerAction {
    Ps {
        all: bool,
        limit: Option<i32>,
        filters: Option<HashMap<String, Vec<String>>>,
    },
    Inspect {
        container: String,
    },
    Logs {
        container: String,
        tail: Option<String>,
        stdout: bool,
        stderr: bool,
        timestamps: bool,
        since: Option<i32>,
    },
    Stats {
        container: String,
    },
    Top {
        container: String,
        ps_args: Option<String>,
    },
    Run {
        container: String,
        cmd: Vec<String>,
        user: Option<String>,
        working_dir: Option<String>,
        env: Option<Vec<String>>,
        privileged: bool,
    },
    Restart {
        container: String,
        timeout_secs: Option<i32>,
    },
}

impl DockerAction {
    /// Read-only actions can run without `allow_write=true`. Write
    /// actions are gated by the orchestrator.
    pub fn is_readonly(&self) -> bool {
        matches!(
            self,
            DockerAction::Ps { .. }
                | DockerAction::Inspect { .. }
                | DockerAction::Logs { .. }
                | DockerAction::Stats { .. }
                | DockerAction::Top { .. }
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            DockerAction::Ps { .. } => "ps",
            DockerAction::Inspect { .. } => "inspect",
            DockerAction::Logs { .. } => "logs",
            DockerAction::Stats { .. } => "stats",
            DockerAction::Top { .. } => "top",
            DockerAction::Run { .. } => "run",
            DockerAction::Restart { .. } => "restart",
        }
    }
}

fn svc_err(action: &str, e: bollard::errors::Error) -> Error {
    Error::Service(format!("docker {action} failed: {e}"))
}

// ----- Actions ---------------------------------------------------------

pub async fn do_ps(
    docker: &Docker,
    all: bool,
    limit: Option<i32>,
    filters: Option<HashMap<String, Vec<String>>>,
) -> Result<ExecutionResult> {
    let mut builder = ListContainersOptionsBuilder::new().all(all);
    if let Some(n) = limit {
        builder = builder.limit(n);
    }
    if let Some(f) = filters {
        let borrowed: HashMap<&str, Vec<&str>> = f
            .iter()
            .map(|(k, v)| (k.as_str(), v.iter().map(|s| s.as_str()).collect()))
            .collect();
        builder = builder.filters(&borrowed);
    }
    let opts = builder.build();
    let containers = docker
        .list_containers(Some(opts))
        .await
        .map_err(|e| svc_err("ps", e))?;

    let columns = vec![
        "id".to_string(),
        "image".to_string(),
        "names".to_string(),
        "state".to_string(),
        "status".to_string(),
        "ports".to_string(),
    ];
    let rows: Vec<Vec<String>> = containers
        .iter()
        .map(|c| {
            let id = c.id.as_deref().unwrap_or("").chars().take(12).collect();
            let image = c.image.clone().unwrap_or_default();
            let names = c
                .names
                .as_ref()
                .map(|v| {
                    v.iter()
                        .map(|n| n.trim_start_matches('/'))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            let state = c
                .state
                .as_ref()
                .map(|s| format!("{s:?}").to_lowercase())
                .unwrap_or_default();
            let status = c.status.clone().unwrap_or_default();
            let ports = c
                .ports
                .as_ref()
                .map(|v| {
                    v.iter()
                        .filter_map(|p| {
                            let public = p.public_port?;
                            Some(format!("{}->{}/{:?}", public, p.private_port, p.typ?))
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            vec![id, image, names, state, status, ports]
        })
        .collect();
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(columns, rows, affected))
}

pub async fn do_inspect(docker: &Docker, container: &str) -> Result<ExecutionResult> {
    let info = docker
        .inspect_container(container, None)
        .await
        .map_err(|e| svc_err("inspect", e))?;
    let json = serde_json::to_string_pretty(&info)
        .map_err(|e| Error::Service(format!("inspect serialize failed: {e}")))?;
    Ok(ExecutionResult::new(
        vec!["inspect".to_string()],
        vec![vec![json]],
        1,
    ))
}

pub async fn do_logs(
    docker: &Docker,
    container: &str,
    tail: Option<&str>,
    stdout: bool,
    stderr: bool,
    timestamps: bool,
    since: Option<i32>,
) -> Result<ExecutionResult> {
    let mut builder = LogsOptionsBuilder::new()
        .follow(false)
        .stdout(stdout)
        .stderr(stderr)
        .timestamps(timestamps)
        .tail(tail.unwrap_or("100"));
    if let Some(s) = since {
        builder = builder.since(s);
    }
    let opts = builder.build();
    let mut stream = docker.logs(container, Some(opts));
    let mut combined = String::new();
    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| svc_err("logs", e))?;
        combined.push_str(&format_log_chunk(&chunk));
    }
    Ok(ExecutionResult::new(
        vec!["logs".to_string()],
        vec![vec![combined]],
        1,
    ))
}

fn format_log_chunk(chunk: &LogOutput) -> String {
    let bytes = match chunk {
        LogOutput::StdErr { message }
        | LogOutput::StdOut { message }
        | LogOutput::StdIn { message }
        | LogOutput::Console { message } => message,
    };
    String::from_utf8_lossy(bytes).to_string()
}

pub async fn do_stats(docker: &Docker, container: &str) -> Result<ExecutionResult> {
    let opts = StatsOptionsBuilder::new()
        .stream(false)
        .one_shot(true)
        .build();
    let mut stream = docker.stats(container, Some(opts));
    let stat = stream
        .next()
        .await
        .ok_or_else(|| Error::Service("docker stats: empty stream".to_string()))?
        .map_err(|e| svc_err("stats", e))?;
    let json = serde_json::to_string_pretty(&stat)
        .map_err(|e| Error::Service(format!("stats serialize failed: {e}")))?;
    Ok(ExecutionResult::new(
        vec!["stats".to_string()],
        vec![vec![json]],
        1,
    ))
}

pub async fn do_top(
    docker: &Docker,
    container: &str,
    ps_args: Option<&str>,
) -> Result<ExecutionResult> {
    let mut builder = TopOptionsBuilder::new();
    if let Some(args) = ps_args {
        builder = builder.ps_args(args);
    }
    let resp = docker
        .top_processes(container, Some(builder.build()))
        .await
        .map_err(|e| svc_err("top", e))?;
    let columns = resp.titles.unwrap_or_default();
    let rows = resp.processes.unwrap_or_default();
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(columns, rows, affected))
}

pub async fn do_run(
    docker: &Docker,
    container: &str,
    cmd: Vec<String>,
    user: Option<String>,
    working_dir: Option<String>,
    env: Option<Vec<String>>,
    privileged: bool,
) -> Result<ExecutionResult> {
    let config: CreateExecOptions<String> = CreateExecOptions {
        cmd: Some(cmd),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        user,
        working_dir,
        env,
        privileged: Some(privileged),
        ..Default::default()
    };
    let created = docker
        .create_exec(container, config)
        .await
        .map_err(|e| svc_err("run/create", e))?;

    let started = docker
        .start_exec(&created.id, None)
        .await
        .map_err(|e| svc_err("run/start", e))?;

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let StartExecResults::Attached { mut output, .. } = started {
        while let Some(item) = output.next().await {
            let chunk = item.map_err(|e| svc_err("run/stream", e))?;
            match chunk {
                LogOutput::StdOut { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                LogOutput::StdErr { message } => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                LogOutput::Console { message } | LogOutput::StdIn { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
            }
        }
    }

    let inspect = docker
        .inspect_exec(&created.id)
        .await
        .map_err(|e| svc_err("run/inspect", e))?;
    let exit_code = inspect.exit_code.unwrap_or(-1).to_string();

    Ok(ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        vec![
            vec!["exit_code".to_string(), exit_code],
            vec!["stdout".to_string(), stdout],
            vec!["stderr".to_string(), stderr],
        ],
        1,
    ))
}

pub async fn do_restart(
    docker: &Docker,
    container: &str,
    timeout_secs: Option<i32>,
) -> Result<ExecutionResult> {
    let mut builder = RestartContainerOptionsBuilder::new();
    if let Some(t) = timeout_secs {
        builder = builder.t(t);
    }
    docker
        .restart_container(container, Some(builder.build()))
        .await
        .map_err(|e| svc_err("restart", e))?;
    Ok(ExecutionResult::new(
        vec!["result".to_string()],
        vec![vec!["restarted".to_string()]],
        1,
    ))
}
