//! Result compression strategies for token-efficient LLM consumption.
//!
//! When SQL results are large, we compress them into formats that preserve
//! semantic meaning while drastically reducing token count. The full data
//! remains available in the UI resource.

use crate::ExecutionResult;
use serde::{Deserialize, Serialize};

/// Compression strategy for large result sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStrategy {
    /// No compression - return full result
    None,
    /// Truncate to first N rows with summary
    Truncate { max_rows: usize },
    /// Statistical summary + sample rows
    Summary { sample_size: usize },
    /// Schema + row count only (most aggressive)
    SchemaOnly,
}

/// Compressed result optimized for LLM understanding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedResult {
    /// Column names and inferred types
    pub schema: Vec<ColumnInfo>,
    /// Total number of rows in the full result
    pub total_rows: usize,
    /// Sample or truncated rows
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sample_rows: Vec<Vec<String>>,
    /// Statistical summary per column (for numeric columns)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub statistics: Vec<ColumnStats>,
    /// Compression metadata
    pub compression_info: CompressionInfo,
    /// Warnings from the original result
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub inferred_type: String,
    pub sample_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub column: String,
    pub min: Option<String>,
    pub max: Option<String>,
    pub distinct_count: usize,
    pub null_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionInfo {
    pub strategy: String,
    pub original_rows: usize,
    pub compressed_rows: usize,
    pub message: String,
}

impl ExecutionResult {
    /// Compress this result for LLM consumption based on size.
    /// Returns a compressed representation that preserves semantic meaning.
    pub fn compress_for_llm(&self) -> CompressedResult {
        let strategy = self.choose_compression_strategy();

        match strategy {
            CompressionStrategy::None => self.to_compressed_full(),
            CompressionStrategy::Truncate { max_rows } => self.to_compressed_truncated(max_rows),
            CompressionStrategy::Summary { sample_size } => self.to_compressed_summary(sample_size),
            CompressionStrategy::SchemaOnly => self.to_compressed_schema_only(),
        }
    }

    /// Choose compression strategy based on result size.
    fn choose_compression_strategy(&self) -> CompressionStrategy {
        let row_count = self.rows.len();

        if row_count <= 20 {
            CompressionStrategy::None
        } else if row_count <= 100 {
            CompressionStrategy::Truncate { max_rows: 20 }
        } else if row_count <= 1000 {
            CompressionStrategy::Summary { sample_size: 10 }
        } else {
            CompressionStrategy::SchemaOnly
        }
    }

    /// Full result (no compression)
    fn to_compressed_full(&self) -> CompressedResult {
        CompressedResult {
            schema: self.infer_schema(),
            total_rows: self.rows.len(),
            sample_rows: self.rows.clone(),
            statistics: vec![],
            compression_info: CompressionInfo {
                strategy: "none".to_string(),
                original_rows: self.rows.len(),
                compressed_rows: self.rows.len(),
                message: format!("Full result: {} rows", self.rows.len()),
            },
            warnings: self.warnings.clone(),
        }
    }

    /// Truncated result (first N rows)
    fn to_compressed_truncated(&self, max_rows: usize) -> CompressedResult {
        let sample_rows: Vec<Vec<String>> = self.rows.iter().take(max_rows).cloned().collect();

        CompressedResult {
            schema: self.infer_schema(),
            total_rows: self.rows.len(),
            sample_rows,
            statistics: vec![],
            compression_info: CompressionInfo {
                strategy: "truncate".to_string(),
                original_rows: self.rows.len(),
                compressed_rows: max_rows.min(self.rows.len()),
                message: format!(
                    "Showing first {} of {} rows. Full data in UI.",
                    max_rows.min(self.rows.len()),
                    self.rows.len()
                ),
            },
            warnings: self.warnings.clone(),
        }
    }

    /// Statistical summary + sample rows
    fn to_compressed_summary(&self, sample_size: usize) -> CompressedResult {
        let sample_rows = self.sample_rows(sample_size);
        let statistics = self.compute_statistics();

        CompressedResult {
            schema: self.infer_schema(),
            total_rows: self.rows.len(),
            sample_rows,
            statistics,
            compression_info: CompressionInfo {
                strategy: "summary".to_string(),
                original_rows: self.rows.len(),
                compressed_rows: sample_size,
                message: format!(
                    "Statistical summary of {} rows. Showing {} sample rows. Full data in UI.",
                    self.rows.len(),
                    sample_size
                ),
            },
            warnings: self.warnings.clone(),
        }
    }

    /// Schema only (most aggressive)
    fn to_compressed_schema_only(&self) -> CompressedResult {
        CompressedResult {
            schema: self.infer_schema(),
            total_rows: self.rows.len(),
            sample_rows: vec![],
            statistics: self.compute_statistics(),
            compression_info: CompressionInfo {
                strategy: "schema_only".to_string(),
                original_rows: self.rows.len(),
                compressed_rows: 0,
                message: format!(
                    "Schema and statistics for {} rows. No sample data shown. Full data in UI.",
                    self.rows.len()
                ),
            },
            warnings: self.warnings.clone(),
        }
    }

    /// Infer column types and get sample values
    fn infer_schema(&self) -> Vec<ColumnInfo> {
        self.columns
            .iter()
            .enumerate()
            .map(|(idx, col_name)| {
                let sample_values: Vec<String> = self
                    .rows
                    .iter()
                    .take(3)
                    .filter_map(|row| row.get(idx).cloned())
                    .collect();

                let inferred_type = self.infer_column_type(idx);

                ColumnInfo {
                    name: col_name.clone(),
                    inferred_type,
                    sample_values,
                }
            })
            .collect()
    }

    /// Infer column type from values
    fn infer_column_type(&self, col_idx: usize) -> String {
        let mut has_null = false;
        let mut all_numeric = true;
        let mut all_int = true;
        let mut max_len = 0;

        for row in self.rows.iter().take(100) {
            if let Some(val) = row.get(col_idx) {
                if val == "NULL" {
                    has_null = true;
                    continue;
                }

                max_len = max_len.max(val.len());

                if all_numeric {
                    if val.parse::<f64>().is_err() {
                        all_numeric = false;
                        all_int = false;
                    } else if all_int && val.parse::<i64>().is_err() {
                        all_int = false;
                    }
                }
            }
        }

        let base_type = if all_int {
            "integer"
        } else if all_numeric {
            "numeric"
        } else if max_len > 100 {
            "text(long)"
        } else {
            "text"
        };

        if has_null {
            format!("{}?", base_type)
        } else {
            base_type.to_string()
        }
    }

    /// Sample rows evenly distributed
    fn sample_rows(&self, sample_size: usize) -> Vec<Vec<String>> {
        if self.rows.len() <= sample_size {
            return self.rows.clone();
        }

        let step = self.rows.len() / sample_size;
        self.rows
            .iter()
            .step_by(step)
            .take(sample_size)
            .cloned()
            .collect()
    }

    /// Compute statistics for each column
    fn compute_statistics(&self) -> Vec<ColumnStats> {
        self.columns
            .iter()
            .enumerate()
            .map(|(idx, col_name)| {
                let mut values: Vec<&String> = vec![];
                let mut null_count = 0;

                for row in &self.rows {
                    if let Some(val) = row.get(idx) {
                        if val == "NULL" {
                            null_count += 1;
                        } else {
                            values.push(val);
                        }
                    }
                }

                let mut unique_values: Vec<&String> = values.clone();
                unique_values.sort();
                unique_values.dedup();

                let (min, max) = if values.is_empty() {
                    (None, None)
                } else {
                    // Try numeric comparison first
                    if let (Some(min_num), Some(max_num)) = (
                        values
                            .iter()
                            .filter_map(|v| v.parse::<f64>().ok())
                            .min_by(|a, b| a.partial_cmp(b).unwrap()),
                        values
                            .iter()
                            .filter_map(|v| v.parse::<f64>().ok())
                            .max_by(|a, b| a.partial_cmp(b).unwrap()),
                    ) {
                        (Some(min_num.to_string()), Some(max_num.to_string()))
                    } else {
                        // Fall back to string comparison
                        (
                            values.iter().min().map(|s| s.to_string()),
                            values.iter().max().map(|s| s.to_string()),
                        )
                    }
                };

                ColumnStats {
                    column: col_name.clone(),
                    min,
                    max,
                    distinct_count: unique_values.len(),
                    null_count,
                }
            })
            .collect()
    }
}
