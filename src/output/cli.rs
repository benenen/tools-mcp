use crate::output::ExecutionResult;
use comfy_table::{Table, presets::UTF8_FULL};

pub struct CliFormatter;

impl CliFormatter {
    pub fn format(result: &ExecutionResult) -> String {
        if result.rows.is_empty() {
            return format!("Query OK, {} rows affected", result.affected_rows);
        }

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(result.columns.iter());

        for row in &result.rows {
            table.add_row(row.iter());
        }

        format!("{}\n\n{} rows in set", table, result.affected_rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::ExecutionResult;

    #[test]
    fn test_format_table() {
        let result = ExecutionResult::new(
            vec!["id".to_string(), "name".to_string()],
            vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ],
            2,
        );

        let output = CliFormatter::format(&result);
        assert!(output.contains("id"));
        assert!(output.contains("name"));
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
    }
}
