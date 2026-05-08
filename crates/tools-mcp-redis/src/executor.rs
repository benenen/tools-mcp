use redis::{Value, cmd};
use tools_mcp_core::{Error, ExecutionResult, Result};

use crate::connection::RedisConnection;

pub struct RedisExecutor;

impl RedisExecutor {
    /// Run `command_str` (e.g. `"GET foo"` or `"HSET h f1 v1 f2 v2"`) against
    /// `conn` and return the result mapped into an `ExecutionResult`.
    pub async fn run(conn: &mut RedisConnection, command_str: &str) -> Result<ExecutionResult> {
        let tokens = shlex::split(command_str).ok_or_else(|| {
            Error::Execution(format!(
                "failed to parse Redis command (unbalanced quotes?): {command_str}"
            ))
        })?;
        let (cmd_name, args) = tokens
            .split_first()
            .ok_or_else(|| Error::Execution("empty Redis command".to_string()))?;

        let redis_conn = conn.get_conn().await?;
        let mut redis_cmd = cmd(cmd_name);
        for arg in args {
            redis_cmd.arg(arg);
        }

        let value: Value = redis_cmd
            .query_async(redis_conn)
            .await
            .map_err(|e: redis::RedisError| Error::Service(format!("Redis: {e}")))?;

        Ok(value_to_result(value))
    }
}

/// Map a redis `Value` into an `ExecutionResult`.
///
/// redis 0.30.0 variant names (verified from source):
///   - `Value::BulkString(Vec<u8>)` — binary-safe bulk string
///   - `Value::Array(Vec<Value>)` — ordered array
///   - `Value::SimpleString(String)` — simple (inline) string
///   - `Value::Nil`, `Value::Int(i64)`, `Value::Okay` — as documented
///   - `Value::Double`, `Value::Boolean`, `Value::Map`, `Value::Set`,
///     `Value::VerbatimString`, `Value::BigNumber`, `Value::Push`,
///     `Value::Attribute`, `Value::ServerError` — fall through to Debug format
fn value_to_result(value: Value) -> ExecutionResult {
    match value {
        Value::Nil => ExecutionResult::new(vec!["result".to_string()], vec![], 0),
        Value::Int(i) => single_cell(i.to_string()),
        Value::BulkString(b) => single_cell(String::from_utf8_lossy(&b).to_string()),
        Value::SimpleString(s) => single_cell(s),
        Value::Okay => single_cell("OK".to_string()),
        Value::Array(items) => {
            let rows: Vec<Vec<String>> = items.into_iter().map(value_to_cell_row).collect();
            let affected = rows.len() as u64;
            ExecutionResult::new(vec!["result".to_string()], rows, affected)
        }
        other => single_cell(format!("{other:?}")),
    }
}

fn single_cell(text: String) -> ExecutionResult {
    ExecutionResult::new(vec!["result".to_string()], vec![vec![text]], 1)
}

/// Recursive helper for nested Array elements: each element becomes one cell of one row.
fn value_to_cell_row(v: Value) -> Vec<String> {
    let cell = match v {
        Value::Nil => "nil".to_string(),
        Value::Int(i) => i.to_string(),
        Value::BulkString(b) => String::from_utf8_lossy(&b).to_string(),
        Value::SimpleString(s) => s,
        Value::Okay => "OK".to_string(),
        other => format!("{other:?}"),
    };
    vec![cell]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_nil_maps_to_empty_rows() {
        let r = value_to_result(Value::Nil);
        assert_eq!(r.columns, vec!["result".to_string()]);
        assert!(r.rows.is_empty());
        assert_eq!(r.affected_rows, 0);
    }

    #[test]
    fn test_value_int_maps_to_single_cell() {
        let r = value_to_result(Value::Int(42));
        assert_eq!(r.rows, vec![vec!["42".to_string()]]);
        assert_eq!(r.affected_rows, 1);
    }

    #[test]
    fn test_value_bulk_string_maps_to_single_cell() {
        // Value::BulkString is the redis 0.30.0 bulk-string variant.
        let r = value_to_result(Value::BulkString(b"hello".to_vec()));
        assert_eq!(r.rows, vec![vec!["hello".to_string()]]);
        assert_eq!(r.affected_rows, 1);
    }

    #[test]
    fn test_value_okay_maps_to_ok() {
        let r = value_to_result(Value::Okay);
        assert_eq!(r.rows, vec![vec!["OK".to_string()]]);
    }

    #[test]
    fn test_value_array_maps_to_one_row_per_item() {
        // Value::Array is the redis 0.30.0 array variant.
        let r = value_to_result(Value::Array(vec![
            Value::BulkString(b"foo".to_vec()),
            Value::BulkString(b"bar".to_vec()),
            Value::Int(7),
        ]));
        assert_eq!(
            r.rows,
            vec![
                vec!["foo".to_string()],
                vec!["bar".to_string()],
                vec!["7".to_string()],
            ]
        );
        assert_eq!(r.affected_rows, 3);
    }
}
