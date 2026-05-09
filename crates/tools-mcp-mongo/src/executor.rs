use crate::connection::MongoConnection;
use mongodb::bson::Document;
use tools_mcp_core::{Error, ExecutionResult, Result};

pub struct MongoExecutor;

impl MongoExecutor {
    /// Parse `command_str` as JSON, convert to BSON, run on the configured
    /// database via `run_command`, and serialize the result Document back
    /// to JSON for an ExecutionResult.
    pub async fn execute(conn: &mut MongoConnection, command_str: &str) -> Result<ExecutionResult> {
        let json: serde_json::Value = serde_json::from_str(command_str)
            .map_err(|e| Error::Execution(format!("failed to parse Mongo command as JSON: {e}")))?;

        if !json.is_object() {
            return Err(Error::Execution(
                "Mongo command must be a JSON object".to_string(),
            ));
        }

        let cmd_doc: Document = mongodb::bson::serialize_to_document(&json).map_err(|e| {
            Error::Execution(format!("failed to convert command JSON to BSON: {e}"))
        })?;

        let client = conn.client()?;
        let db = client.database(conn.database_name());
        let result_doc: Document = db
            .run_command(cmd_doc)
            .await
            .map_err(|e| Error::Service(format!("Mongo run_command: {e}")))?;

        let result_json = serde_json::to_string(&result_doc)
            .map_err(|e| Error::Service(format!("Mongo result serialization: {e}")))?;

        Ok(ExecutionResult::new(
            vec!["result".to_string()],
            vec![vec![result_json]],
            1,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mongo_executor_new() {
        let _e = MongoExecutor;
    }
}
