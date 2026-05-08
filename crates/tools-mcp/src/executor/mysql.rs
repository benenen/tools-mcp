use crate::connection::MySQLConnection;
use crate::error::Result;
use crate::output::ExecutionResult;
use mysql_async::{Row, Value, prelude::*};

pub struct MySQLExecutor;

impl MySQLExecutor {
    pub async fn execute(conn: &mut MySQLConnection, query: &str) -> Result<ExecutionResult> {
        let mysql_conn = conn.get_conn().await?;

        let result: Vec<Row> = mysql_conn.query(query).await?;

        if result.is_empty() {
            return Ok(ExecutionResult::new(vec![], vec![], 0));
        }

        let columns: Vec<String> = result[0]
            .columns_ref()
            .iter()
            .map(|col| col.name_str().to_string())
            .collect();

        let rows: Vec<Vec<String>> = result
            .iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| Self::value_to_string(row.as_ref(i).unwrap_or(&Value::NULL)))
                    .collect()
            })
            .collect();

        let affected_rows = rows.len() as u64;

        Ok(ExecutionResult::new(columns, rows, affected_rows))
    }

    fn value_to_string(value: &Value) -> String {
        match value {
            Value::NULL => "NULL".to_string(),
            Value::Bytes(b) => String::from_utf8_lossy(b).to_string(),
            Value::Int(i) => i.to_string(),
            Value::UInt(u) => u.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Double(d) => d.to_string(),
            Value::Date(y, m, d, h, min, s, _) => {
                format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, s)
            }
            Value::Time(neg, days, h, m, s, _) => {
                if *neg {
                    format!("-{:02}:{:02}:{:02}", days * 24 + *h as u32, m, s)
                } else {
                    format!("{:02}:{:02}:{:02}", days * 24 + *h as u32, m, s)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_executor_new() {
        let _executor = MySQLExecutor;
    }
}
