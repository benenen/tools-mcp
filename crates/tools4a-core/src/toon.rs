//! TOON (Token-Optimized Object Notation) format support.
//!
//! TOON is a data serialization format designed specifically for LLM contexts.
//! It reduces token usage by 30-60% compared to JSON by using indentation-based
//! structure and tabular arrays.
//!
//! This module uses the official `toon` crate: https://github.com/toon-format/toon-rust

use crate::{CompressedResult, ExecutionResult};

/// Convert ExecutionResult to TOON format using the official toon crate.
pub fn to_toon(result: &ExecutionResult) -> String {
    // Convert to serde_json::Value first
    let json_value = serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({}));

    // Use toon::encode to convert to TOON format
    toon::encode(&json_value, None)
}

/// Convert CompressedResult to TOON format using the official toon crate.
pub fn compressed_to_toon(result: &CompressedResult) -> String {
    // Convert to serde_json::Value first
    let json_value = serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({}));

    // Use toon::encode to convert to TOON format
    toon::encode(&json_value, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_toon() {
        let result = ExecutionResult::new(
            vec!["id".to_string(), "name".to_string()],
            vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ],
            2,
        );

        let toon = to_toon(&result);
        // TOON format should be more compact than JSON
        assert!(!toon.is_empty());
        assert!(toon.contains("id"));
        assert!(toon.contains("name"));
    }

    #[test]
    fn test_compressed_toon() {
        let result = ExecutionResult::new(
            vec!["id".to_string(), "value".to_string()],
            vec![
                vec!["1".to_string(), "100".to_string()],
                vec!["2".to_string(), "200".to_string()],
            ],
            2,
        );

        let compressed = result.compress_for_llm();
        let toon = compressed_to_toon(&compressed);

        // Should contain schema and statistics
        assert!(!toon.is_empty());
        assert!(toon.contains("schema") || toon.contains("total_rows"));
    }
}
