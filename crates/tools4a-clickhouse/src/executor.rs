//! Executes a single SQL query against ClickHouse over HTTP and converts
//! the JSONCompact response into our column/row `ExecutionResult` shape.
//!
//! JSONCompact body shape:
//! ```json
//! {
//!   "meta":[{"name":"id","type":"UInt32"},{"name":"name","type":"String"}],
//!   "data":[[1,"Alice"],[2,"Bob"]],
//!   "rows":2,
//!   "statistics":{...}
//! }
//! ```

use crate::connection::ClickhouseConnection;
use serde::Deserialize;
use serde_json::Value;
use tools4a_core::{Error, ExecutionResult, Result};

pub struct ClickhouseExecutor;

#[derive(Debug, Deserialize)]
struct JsonCompactResponse {
    #[serde(default)]
    meta: Vec<MetaCol>,
    #[serde(default)]
    data: Vec<Vec<Value>>,
    #[serde(default)]
    rows: u64,
}

#[derive(Debug, Deserialize)]
struct MetaCol {
    name: String,
    #[serde(rename = "type")]
    _ty: String,
}

impl ClickhouseExecutor {
    pub async fn execute(conn: &ClickhouseConnection, query: &str) -> Result<ExecutionResult> {
        let client = conn.client()?;

        let bytes = client
            .query(query)
            .fetch_bytes("JSONCompact")
            .map_err(|e| Error::Service(format!("Clickhouse query: {e}")))?
            .collect()
            .await
            .map_err(|e| Error::Service(format!("Clickhouse fetch: {e}")))?;

        // Empty body — successful no-result statement (e.g. DDL).
        if bytes.is_empty() {
            return Ok(ExecutionResult::new(vec![], vec![], 0));
        }

        let parsed: JsonCompactResponse = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Service(format!("Clickhouse parse JSONCompact: {e}")))?;

        let columns: Vec<String> = parsed.meta.iter().map(|c| c.name.clone()).collect();
        let str_rows: Vec<Vec<String>> = parsed
            .data
            .iter()
            .map(|row| row.iter().map(value_to_string).collect())
            .collect();

        let n = if parsed.rows > 0 {
            parsed.rows
        } else {
            str_rows.len() as u64
        };
        Ok(ExecutionResult::new(columns, str_rows, n))
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        // Array(...) / Map(...) / Tuple(...) columns serialize as compact JSON.
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jsoncompact_response() {
        let body = r#"{
            "meta":[{"name":"id","type":"UInt32"},{"name":"name","type":"String"}],
            "data":[[1,"Alice"],[2,"Bob"]],
            "rows":2
        }"#;
        let parsed: JsonCompactResponse = serde_json::from_slice(body.as_bytes()).unwrap();
        assert_eq!(parsed.meta.len(), 2);
        assert_eq!(parsed.meta[0].name, "id");
        assert_eq!(parsed.data.len(), 2);
        assert_eq!(parsed.rows, 2);
        assert_eq!(value_to_string(&parsed.data[0][0]), "1");
        assert_eq!(value_to_string(&parsed.data[0][1]), "Alice");
    }

    #[test]
    fn null_renders_as_null_string() {
        assert_eq!(value_to_string(&Value::Null), "NULL");
    }

    #[test]
    fn bool_renders_as_true_false() {
        assert_eq!(value_to_string(&Value::Bool(true)), "true");
        assert_eq!(value_to_string(&Value::Bool(false)), "false");
    }

    #[test]
    fn array_renders_as_compact_json() {
        let v: Value = serde_json::from_str("[1,2,3]").unwrap();
        assert_eq!(value_to_string(&v), "[1,2,3]");
    }

    #[test]
    fn missing_data_field_yields_empty_rows() {
        let body = r#"{"meta":[{"name":"x","type":"UInt8"}]}"#;
        let parsed: JsonCompactResponse = serde_json::from_slice(body.as_bytes()).unwrap();
        assert!(parsed.data.is_empty());
        assert_eq!(parsed.rows, 0);
    }
}
