//! Single dispatcher that switches on a `DockerAction` and calls the
//! matching action function with a connected `bollard::Docker`. Plays
//! the same role as `tools4a_mysql::execute` does for that crate.

use crate::actions::{self, DockerAction};
use bollard::Docker;
use tools4a_core::{ExecutionResult, Result};

/// Dispatch `action` against `docker`. Read-only / write gating is
/// already enforced by the caller (`DockerOrchestrator`).
pub async fn run(docker: &Docker, action: DockerAction) -> Result<ExecutionResult> {
    match action {
        DockerAction::Ps {
            all,
            limit,
            filters,
        } => actions::do_ps(docker, all, limit, filters).await,
        DockerAction::Inspect { container } => actions::do_inspect(docker, &container).await,
        DockerAction::Logs {
            container,
            tail,
            stdout,
            stderr,
            timestamps,
            since,
        } => {
            actions::do_logs(
                docker,
                &container,
                tail.as_deref(),
                stdout,
                stderr,
                timestamps,
                since,
            )
            .await
        }
        DockerAction::Stats { container } => actions::do_stats(docker, &container).await,
        DockerAction::Top { container, ps_args } => {
            actions::do_top(docker, &container, ps_args.as_deref()).await
        }
        DockerAction::Run {
            container,
            cmd,
            user,
            working_dir,
            env,
            privileged,
        } => actions::do_run(docker, &container, cmd, user, working_dir, env, privileged).await,
        DockerAction::Restart {
            container,
            timeout_secs,
        } => actions::do_restart(docker, &container, timeout_secs).await,
    }
}
