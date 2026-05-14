//! Five action functions that hit the RabbitMQ management API and
//! shape the response into an `ExecutionResult`.

use crate::connection::{RabbitmqConnection, encode_vhost};
use serde_json::{Value, json};
use tools4a_core::{Error, ExecutionResult, Result};

/// One of the five supported actions. The orchestrator constructs
/// this; `run::run` dispatches.
#[derive(Debug, Clone)]
pub enum RabbitmqAction {
    ListQueues {
        vhost: Option<String>,
        name_pattern: Option<String>,
        limit: Option<usize>,
    },
    QueueInfo {
        vhost: String,
        name: String,
    },
    GetMessages {
        vhost: String,
        queue: String,
        count: usize,
        truncate_bytes: Option<usize>,
    },
    ListBindings {
        vhost: Option<String>,
        source: Option<String>,
        destination: Option<String>,
    },
    Overview,
}

impl RabbitmqAction {
    pub fn name(&self) -> &'static str {
        match self {
            RabbitmqAction::ListQueues { .. } => "list_queues",
            RabbitmqAction::QueueInfo { .. } => "queue_info",
            RabbitmqAction::GetMessages { .. } => "get_messages",
            RabbitmqAction::ListBindings { .. } => "list_bindings",
            RabbitmqAction::Overview => "overview",
        }
    }
}

// ----- list_queues ----------------------------------------------------

pub async fn do_list_queues(
    conn: &RabbitmqConnection,
    vhost: Option<&str>,
    name_pattern: Option<&str>,
    limit: Option<usize>,
) -> Result<ExecutionResult> {
    let path = match vhost {
        Some(v) => format!("/api/queues/{}", encode_vhost(v)),
        None => "/api/queues".to_string(),
    };
    let v = conn.get_json(&path).await?;
    let queues = v.as_array().ok_or_else(|| {
        Error::Service(format!("rabbitmq {path}: expected JSON array, got {v:?}"))
    })?;

    let columns = vec![
        "name".to_string(),
        "vhost".to_string(),
        "state".to_string(),
        "messages_ready".to_string(),
        "messages_unacked".to_string(),
        "messages_total".to_string(),
        "consumers".to_string(),
        "publish_rate".to_string(),
        "deliver_get_rate".to_string(),
    ];

    let mut rows: Vec<Vec<String>> = Vec::new();
    for q in queues {
        let name = q.get("name").and_then(|x| x.as_str()).unwrap_or("");
        if let Some(pat) = name_pattern
            && !pattern_matches(name, pat)
        {
            continue;
        }
        let vhost_s = q.get("vhost").and_then(|x| x.as_str()).unwrap_or("");
        let state = q.get("state").and_then(|x| x.as_str()).unwrap_or("");
        let ready = num(q.get("messages_ready"));
        let unacked = num(q.get("messages_unacknowledged"));
        let total = num(q.get("messages"));
        let consumers = num(q.get("consumers"));
        let publish_rate = q
            .pointer("/message_stats/publish_details/rate")
            .and_then(|x| x.as_f64())
            .map(|r| format!("{r:.2}"))
            .unwrap_or_default();
        let deliver_rate = q
            .pointer("/message_stats/deliver_get_details/rate")
            .and_then(|x| x.as_f64())
            .map(|r| format!("{r:.2}"))
            .unwrap_or_default();
        rows.push(vec![
            name.to_string(),
            vhost_s.to_string(),
            state.to_string(),
            ready,
            unacked,
            total,
            consumers,
            publish_rate,
            deliver_rate,
        ]);
        if let Some(l) = limit
            && rows.len() >= l
        {
            break;
        }
    }
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(columns, rows, affected))
}

// ----- queue_info ----------------------------------------------------

pub async fn do_queue_info(
    conn: &RabbitmqConnection,
    vhost: &str,
    name: &str,
) -> Result<ExecutionResult> {
    let path = format!(
        "/api/queues/{}/{}",
        encode_vhost(vhost),
        urlencoding::encode(name)
    );
    let v = conn.get_json(&path).await?;
    let pretty = serde_json::to_string_pretty(&v)
        .map_err(|e| Error::Service(format!("queue_info serialize: {e}")))?;
    Ok(ExecutionResult::new(
        vec!["queue_info".to_string()],
        vec![vec![pretty]],
        1,
    ))
}

// ----- get_messages (non-destructive peek) ---------------------------

pub async fn do_get_messages(
    conn: &RabbitmqConnection,
    vhost: &str,
    queue: &str,
    count: usize,
    truncate_bytes: Option<usize>,
) -> Result<ExecutionResult> {
    let path = format!(
        "/api/queues/{}/{}/get",
        encode_vhost(vhost),
        urlencoding::encode(queue)
    );
    let mut body = json!({
        "count": count,
        // Peek-only: server requeues the message after we look at it.
        "ackmode": "ack_requeue_true",
        "encoding": "auto",
    });
    if let Some(t) = truncate_bytes {
        body["truncate"] = json!(t);
    }
    let v = conn.post_json(&path, &body).await?;
    let messages = v
        .as_array()
        .ok_or_else(|| Error::Service(format!("rabbitmq {path}: expected array, got {v:?}")))?;

    let columns = vec![
        "redelivered".to_string(),
        "exchange".to_string(),
        "routing_key".to_string(),
        "payload_encoding".to_string(),
        "payload_size".to_string(),
        "payload".to_string(),
    ];
    let rows: Vec<Vec<String>> = messages
        .iter()
        .map(|m| {
            let redelivered = m
                .get("redelivered")
                .and_then(|x| x.as_bool())
                .map(|b| b.to_string())
                .unwrap_or_default();
            let exchange = m
                .get("exchange")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let routing_key = m
                .get("routing_key")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let payload_encoding = m
                .get("payload_encoding")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let payload_size = num(m.get("payload_bytes"));
            let payload = m
                .get("payload")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            vec![
                redelivered,
                exchange,
                routing_key,
                payload_encoding,
                payload_size,
                payload,
            ]
        })
        .collect();
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(columns, rows, affected))
}

// ----- list_bindings -------------------------------------------------

pub async fn do_list_bindings(
    conn: &RabbitmqConnection,
    vhost: Option<&str>,
    source_filter: Option<&str>,
    destination_filter: Option<&str>,
) -> Result<ExecutionResult> {
    let path = match vhost {
        Some(v) => format!("/api/bindings/{}", encode_vhost(v)),
        None => "/api/bindings".to_string(),
    };
    let v = conn.get_json(&path).await?;
    let bindings = v.as_array().ok_or_else(|| {
        Error::Service(format!("rabbitmq {path}: expected JSON array, got {v:?}"))
    })?;

    let columns = vec![
        "source".to_string(),
        "destination".to_string(),
        "destination_type".to_string(),
        "routing_key".to_string(),
        "vhost".to_string(),
        "arguments".to_string(),
    ];
    let rows: Vec<Vec<String>> = bindings
        .iter()
        .filter(|b| {
            let src = b.get("source").and_then(|x| x.as_str()).unwrap_or("");
            let dest = b.get("destination").and_then(|x| x.as_str()).unwrap_or("");
            source_filter.is_none_or(|s| pattern_matches(src, s))
                && destination_filter.is_none_or(|d| pattern_matches(dest, d))
        })
        .map(|b| {
            let source = b
                .get("source")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let destination = b
                .get("destination")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let dest_type = b
                .get("destination_type")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let routing_key = b
                .get("routing_key")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let vhost_s = b
                .get("vhost")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let args = b
                .get("arguments")
                .map(|a| a.to_string())
                .unwrap_or_default();
            vec![source, destination, dest_type, routing_key, vhost_s, args]
        })
        .collect();
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(columns, rows, affected))
}

// ----- overview ------------------------------------------------------

pub async fn do_overview(conn: &RabbitmqConnection) -> Result<ExecutionResult> {
    let v = conn.get_json("/api/overview").await?;
    let columns = vec!["field".to_string(), "value".to_string()];
    let rows = vec![
        cell("rabbitmq_version", v.pointer("/rabbitmq_version")),
        cell("erlang_version", v.pointer("/erlang_version")),
        cell("cluster_name", v.pointer("/cluster_name")),
        cell("product_version", v.pointer("/product_version")),
        cell("node", v.pointer("/node")),
        cell("object_totals.queues", v.pointer("/object_totals/queues")),
        cell(
            "object_totals.exchanges",
            v.pointer("/object_totals/exchanges"),
        ),
        cell(
            "object_totals.connections",
            v.pointer("/object_totals/connections"),
        ),
        cell(
            "object_totals.channels",
            v.pointer("/object_totals/channels"),
        ),
        cell(
            "object_totals.consumers",
            v.pointer("/object_totals/consumers"),
        ),
        cell("queue_totals.messages", v.pointer("/queue_totals/messages")),
        cell(
            "queue_totals.messages_ready",
            v.pointer("/queue_totals/messages_ready"),
        ),
        cell(
            "queue_totals.messages_unacknowledged",
            v.pointer("/queue_totals/messages_unacknowledged"),
        ),
        cell(
            "message_stats.publish_details.rate",
            v.pointer("/message_stats/publish_details/rate"),
        ),
        cell(
            "message_stats.deliver_get_details.rate",
            v.pointer("/message_stats/deliver_get_details/rate"),
        ),
    ];
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(columns, rows, affected))
}

// ----- helpers --------------------------------------------------------

fn num(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn cell(label: &str, v: Option<&Value>) -> Vec<String> {
    let value = match v {
        Some(Value::Null) | None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    vec![label.to_string(), value]
}

/// Simple glob: `*` matches any (possibly empty) substring. No `?` /
/// character classes / escaping. Returns true if the whole pattern
/// matches the whole input. Reused for queue / binding name filters.
fn pattern_matches(input: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return input == pattern;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut cursor = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !input[cursor..].starts_with(part) {
                return false;
            }
            cursor += part.len();
        } else if i == parts.len() - 1 {
            return input[cursor..].ends_with(part);
        } else {
            match input[cursor..].find(part) {
                Some(idx) => cursor += idx + part.len(),
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_exact() {
        assert!(pattern_matches("foo", "foo"));
        assert!(!pattern_matches("foo", "bar"));
    }

    #[test]
    fn pattern_prefix() {
        assert!(pattern_matches("ai_teacher_behavior_1", "ai_teacher*"));
        assert!(!pattern_matches("other_queue", "ai_teacher*"));
    }

    #[test]
    fn pattern_suffix() {
        assert!(pattern_matches("queue_result", "*_result"));
    }

    #[test]
    fn pattern_middle() {
        assert!(pattern_matches(
            "ai_teacher_behavior_1-2-10.189.109.55",
            "ai_*55"
        ));
        assert!(!pattern_matches("foo_bar", "ai_*55"));
    }

    #[test]
    fn pattern_star_only() {
        assert!(pattern_matches("anything", "*"));
        assert!(pattern_matches("", "*"));
    }
}
