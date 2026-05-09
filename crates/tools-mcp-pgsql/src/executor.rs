use crate::connection::PgsqlConnection;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use tokio_postgres::{Row, types::Type};
use tools_mcp_core::{Error, ExecutionResult, Result};

pub struct PgsqlExecutor;

impl PgsqlExecutor {
    pub async fn execute(conn: &mut PgsqlConnection, query: &str) -> Result<ExecutionResult> {
        let client = conn.client()?;

        let rows: Vec<Row> = client
            .query(query, &[])
            .await
            .map_err(|e| Error::Service(format!("Pgsql query: {e}")))?;

        if rows.is_empty() {
            return Ok(ExecutionResult::new(vec![], vec![], 0));
        }

        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let str_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| Self::col_to_string(row, i))
                    .collect()
            })
            .collect();

        let n = str_rows.len() as u64;
        Ok(ExecutionResult::new(columns, str_rows, n))
    }

    fn col_to_string(row: &Row, i: usize) -> String {
        let col = &row.columns()[i];
        let ty = col.type_();

        macro_rules! show_opt {
            ($t:ty) => {
                match row.try_get::<_, Option<$t>>(i) {
                    Ok(Some(v)) => v.to_string(),
                    Ok(None) => "NULL".to_string(),
                    Err(e) => format!("<{}: {e}>", ty.name()),
                }
            };
        }

        match *ty {
            Type::BOOL => show_opt!(bool),
            Type::INT2 => show_opt!(i16),
            Type::INT4 => show_opt!(i32),
            Type::INT8 => show_opt!(i64),
            Type::FLOAT4 => show_opt!(f32),
            Type::FLOAT8 => show_opt!(f64),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => show_opt!(String),
            Type::DATE => show_opt!(NaiveDate),
            Type::TIME => show_opt!(NaiveTime),
            Type::TIMESTAMP => show_opt!(NaiveDateTime),
            Type::TIMESTAMPTZ => show_opt!(DateTime<Utc>),
            _ => format!("<{}>", ty.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pgsql_executor_new() {
        let _e = PgsqlExecutor;
    }
}
