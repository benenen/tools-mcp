//! Five `McpTool` impls for the RabbitMQ leaf. All share a flattened
//! `RabbitmqConnectionFields` struct that carries host/port/scheme/auth
//! + the standard tunnel fields.

use crate::actions::RabbitmqAction;
use crate::orchestrator::{RabbitmqOrchestrator, RabbitmqRequest, default_port_for};
use async_trait::async_trait;

use schemars::JsonSchema;
use serde::Deserialize;
use tools4a_core::{
    ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

// -- Shared connection fields ----------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct RabbitmqConnectionFields {
    /// Management API host. Required.
    pub host: String,
    /// "http" (default) or "https".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// Management API port. Default 15672 (HTTP) or 15671 (HTTPS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// HTTP basic-auth user. Default "guest".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// HTTP basic-auth password. Default "guest".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Skip TLS cert verification. Default false. Useful for HTTPS with
    /// self-signed certs, or HTTPS through a tunnel where the cert
    /// doesn't cover 127.0.0.1.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub insecure: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,

    /// Per-call timeout (seconds). Default 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

fn build_req(
    conn: RabbitmqConnectionFields,
    action: RabbitmqAction,
) -> Result<(RabbitmqRequest, Option<tools4a_core::TunnelConfig>)> {
    let scheme = conn.scheme.unwrap_or_else(|| "http".to_string());
    let port = conn.port.unwrap_or_else(|| default_port_for(&scheme));
    let user = conn.user.unwrap_or_else(|| "guest".to_string());
    let password = conn.password.unwrap_or_else(|| "guest".to_string());

    let tunnel = build_tunnel_config(
        conn.tunnel,
        conn.ssh_jump,
        conn.ssh_user,
        conn.ssh_password,
        conn.ssh_key_path,
        conn.ssh_port,
    )?;

    let req = RabbitmqRequest {
        action,
        scheme,
        host: conn.host,
        port,
        user,
        password,
        insecure: conn.insecure,
        timeout_secs: conn.timeout_secs,
        max_timeout_secs: None,
    };
    Ok((req, tunnel))
}

// -- rabbitmq_list_queues --------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RabbitmqListQueuesParams {
    /// Restrict to a single vhost. Omit to list across all vhosts the
    /// user can see.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vhost: Option<String>,
    /// Glob filter on queue name (`*` wildcard). e.g. `ai_teacher_*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_pattern: Option<String>,
    /// Hard cap on rows returned (after pattern filter). Useful when the
    /// daemon has thousands of queues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(flatten)]
    pub conn: RabbitmqConnectionFields,
}

pub struct RabbitmqListQueuesMcp;
#[async_trait]
impl McpTool for RabbitmqListQueuesMcp {
    const NAME: &'static str = "rabbitmq_list_queues";
    const DESCRIPTION: &'static str = "List RabbitMQ queues with counts (ready/unacked/total/consumers) and message rates. \
         Read-only. Optional vhost + glob-on-name filter.";
    type Params = RabbitmqListQueuesParams;

    async fn invoke(p: RabbitmqListQueuesParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            RabbitmqAction::ListQueues {
                vhost: p.vhost,
                name_pattern: p.name_pattern,
                limit: p.limit,
            },
        )?;
        RabbitmqOrchestrator::execute(req, tunnel).await
    }
}

// -- rabbitmq_queue_info ---------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RabbitmqQueueInfoParams {
    pub vhost: String,
    pub name: String,
    #[serde(flatten)]
    pub conn: RabbitmqConnectionFields,
}

pub struct RabbitmqQueueInfoMcp;
#[async_trait]
impl McpTool for RabbitmqQueueInfoMcp {
    const NAME: &'static str = "rabbitmq_queue_info";
    const DESCRIPTION: &'static str = "Inspect a single RabbitMQ queue (full JSON: settings, policy, consumer details, stats). \
         Read-only.";
    type Params = RabbitmqQueueInfoParams;

    async fn invoke(p: RabbitmqQueueInfoParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            RabbitmqAction::QueueInfo {
                vhost: p.vhost,
                name: p.name,
            },
        )?;
        RabbitmqOrchestrator::execute(req, tunnel).await
    }
}

// -- rabbitmq_get_messages -------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RabbitmqGetMessagesParams {
    pub vhost: String,
    pub queue: String,
    /// How many messages to fetch. Default 1.
    #[serde(default = "default_count")]
    pub count: usize,
    /// Truncate payload bytes past this length (server-side). Useful for
    /// fat messages where you only want headers/start of body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncate_bytes: Option<usize>,
    #[serde(flatten)]
    pub conn: RabbitmqConnectionFields,
}

fn default_count() -> usize {
    1
}

pub struct RabbitmqGetMessagesMcp;
#[async_trait]
impl McpTool for RabbitmqGetMessagesMcp {
    const NAME: &'static str = "rabbitmq_get_messages";
    const DESCRIPTION: &'static str = "Peek at messages in a queue without consuming them (uses ackmode=ack_requeue_true so the \
         server immediately requeues each message). Returns redelivered flag, exchange, routing \
         key, payload encoding, payload bytes.";
    type Params = RabbitmqGetMessagesParams;

    async fn invoke(p: RabbitmqGetMessagesParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            RabbitmqAction::GetMessages {
                vhost: p.vhost,
                queue: p.queue,
                count: p.count,
                truncate_bytes: p.truncate_bytes,
            },
        )?;
        RabbitmqOrchestrator::execute(req, tunnel).await
    }
}

// -- rabbitmq_list_bindings ------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RabbitmqListBindingsParams {
    /// Restrict to a single vhost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vhost: Option<String>,
    /// Glob filter on source (exchange name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Glob filter on destination (queue or exchange name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(flatten)]
    pub conn: RabbitmqConnectionFields,
}

pub struct RabbitmqListBindingsMcp;
#[async_trait]
impl McpTool for RabbitmqListBindingsMcp {
    const NAME: &'static str = "rabbitmq_list_bindings";
    const DESCRIPTION: &'static str = "List RabbitMQ bindings (source exchange -> destination queue/exchange + routing key). \
         Read-only. Optional vhost + glob filters on source / destination.";
    type Params = RabbitmqListBindingsParams;

    async fn invoke(p: RabbitmqListBindingsParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            RabbitmqAction::ListBindings {
                vhost: p.vhost,
                source: p.source,
                destination: p.destination,
            },
        )?;
        RabbitmqOrchestrator::execute(req, tunnel).await
    }
}

// -- rabbitmq_overview ------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RabbitmqOverviewParams {
    #[serde(flatten)]
    pub conn: RabbitmqConnectionFields,
}

pub struct RabbitmqOverviewMcp;
#[async_trait]
impl McpTool for RabbitmqOverviewMcp {
    const NAME: &'static str = "rabbitmq_overview";
    const DESCRIPTION: &'static str = "Cluster overview (versions, totals of queues/exchanges/connections/channels/consumers, \
         message rates, queue totals). Read-only.";
    type Params = RabbitmqOverviewParams;

    async fn invoke(p: RabbitmqOverviewParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(p.conn, RabbitmqAction::Overview)?;
        RabbitmqOrchestrator::execute(req, tunnel).await
    }
}
