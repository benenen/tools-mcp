//! Dispatcher: takes a `RabbitmqAction` + a connected client and calls
//! the matching action function.

use crate::actions::{self, RabbitmqAction};
use crate::connection::RabbitmqConnection;
use tools4a_core::{ExecutionResult, Result};

pub async fn run(conn: &RabbitmqConnection, action: RabbitmqAction) -> Result<ExecutionResult> {
    match action {
        RabbitmqAction::ListQueues {
            vhost,
            name_pattern,
            limit,
        } => actions::do_list_queues(conn, vhost.as_deref(), name_pattern.as_deref(), limit).await,
        RabbitmqAction::QueueInfo { vhost, name } => {
            actions::do_queue_info(conn, &vhost, &name).await
        }
        RabbitmqAction::GetMessages {
            vhost,
            queue,
            count,
            truncate_bytes,
        } => actions::do_get_messages(conn, &vhost, &queue, count, truncate_bytes).await,
        RabbitmqAction::ListBindings {
            vhost,
            source,
            destination,
        } => {
            actions::do_list_bindings(
                conn,
                vhost.as_deref(),
                source.as_deref(),
                destination.as_deref(),
            )
            .await
        }
        RabbitmqAction::Overview => actions::do_overview(conn).await,
    }
}
