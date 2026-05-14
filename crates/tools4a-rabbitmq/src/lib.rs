//! RabbitMQ Management HTTP API leaf. Five read-only MCP tools:
//! - `rabbitmq_list_queues`
//! - `rabbitmq_queue_info`
//! - `rabbitmq_get_messages` (non-destructive peek)
//! - `rabbitmq_list_bindings`
//! - `rabbitmq_overview`
//!
//! See `docs/superpowers/plans/2026-05-14-tools-mcp-phase17-rabbitmq.md`.

pub mod actions;
pub mod connection;
pub mod mcp;
pub mod orchestrator;
pub mod run;

pub use actions::RabbitmqAction;
pub use connection::RabbitmqConnection;
pub use mcp::{
    RabbitmqConnectionFields, RabbitmqGetMessagesMcp, RabbitmqGetMessagesParams,
    RabbitmqListBindingsMcp, RabbitmqListBindingsParams, RabbitmqListQueuesMcp,
    RabbitmqListQueuesParams, RabbitmqOverviewMcp, RabbitmqOverviewParams, RabbitmqQueueInfoMcp,
    RabbitmqQueueInfoParams,
};
pub use orchestrator::{RabbitmqOrchestrator, RabbitmqRequest};
pub use run::run;
