//! Ten action functions. 6 read + 1 vector search + 3 write (gated by
//! the orchestrator via `allow_write`). Each returns an
//! `ExecutionResult` with the most useful subset of the SDK response.

use milvus::client::Client;
use milvus::data::FieldColumn;
use milvus::query::{QueryOptions, SearchOptions};
use milvus::value::{Value, ValueVec};
use tools4a_core::{Error, ExecutionResult, Result};

/// One of the ten supported actions. The orchestrator dispatches via
/// `run::run`.
#[derive(Debug, Clone)]
pub enum MilvusAction {
    ListDatabases,
    ListCollections,
    DescribeCollection {
        name: String,
    },
    CollectionStats {
        name: String,
    },
    ListPartitions {
        collection: String,
    },
    Query {
        collection: String,
        expr: String,
        output_fields: Vec<String>,
        partition_names: Vec<String>,
        limit: Option<i64>,
        /// When false (default), vector-typed fields render as
        /// `<vec dim=N>` instead of dumping all the floats. Saves
        /// tokens; the user can opt in when they actually need the data.
        include_vectors: bool,
    },
    Search {
        collection: String,
        vectors: Vec<Vec<f32>>,
        metric: String,
        limit: usize,
        output_fields: Vec<String>,
        filter: Option<String>,
        /// Optional explicit vector field name (`anns_field`). Required
        /// when the collection has multiple vector fields.
        anns_field: Option<String>,
        /// See `Query::include_vectors`.
        include_vectors: bool,
    },
    DropCollection {
        name: String,
    },
    LoadCollection {
        name: String,
    },
    ReleaseCollection {
        name: String,
    },
}

impl MilvusAction {
    pub fn name(&self) -> &'static str {
        match self {
            MilvusAction::ListDatabases => "list_databases",
            MilvusAction::ListCollections => "list_collections",
            MilvusAction::DescribeCollection { .. } => "describe_collection",
            MilvusAction::CollectionStats { .. } => "collection_stats",
            MilvusAction::ListPartitions { .. } => "list_partitions",
            MilvusAction::Query { .. } => "query",
            MilvusAction::Search { .. } => "search",
            MilvusAction::DropCollection { .. } => "drop_collection",
            MilvusAction::LoadCollection { .. } => "load_collection",
            MilvusAction::ReleaseCollection { .. } => "release_collection",
        }
    }

    /// Read-only actions can run without `allow_write=true`.
    pub fn is_readonly(&self) -> bool {
        matches!(
            self,
            MilvusAction::ListDatabases
                | MilvusAction::ListCollections
                | MilvusAction::DescribeCollection { .. }
                | MilvusAction::CollectionStats { .. }
                | MilvusAction::ListPartitions { .. }
                | MilvusAction::Query { .. }
                | MilvusAction::Search { .. }
        )
    }
}

fn svc_err(action: &str, e: milvus::error::Error) -> Error {
    Error::Service(format!("milvus {action} failed: {e}"))
}

// ----- list_databases ---------------------------------------------

pub async fn do_list_databases(client: &Client) -> Result<ExecutionResult> {
    let dbs = client
        .list_databases()
        .await
        .map_err(|e| svc_err("list_databases", e))?;
    let rows: Vec<Vec<String>> = dbs.into_iter().map(|d| vec![d]).collect();
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(
        vec!["database".to_string()],
        rows,
        affected,
    ))
}

// ----- list_collections -------------------------------------------

pub async fn do_list_collections(client: &Client) -> Result<ExecutionResult> {
    let names = client
        .list_collections()
        .await
        .map_err(|e| svc_err("list_collections", e))?;
    let rows: Vec<Vec<String>> = names.into_iter().map(|n| vec![n]).collect();
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(
        vec!["collection".to_string()],
        rows,
        affected,
    ))
}

// ----- describe_collection ----------------------------------------

pub async fn do_describe_collection(client: &Client, name: &str) -> Result<ExecutionResult> {
    let desc = client
        .describe_collection(name)
        .await
        .map_err(|e| svc_err("describe_collection", e))?;
    let json = serde_json::json!({
        "collection_name": desc.collection_name,
        "collection_id":   desc.collection_id,
        "shards_num":      desc.shards_num,
        "aliases":         desc.aliases,
        "consistency_level": desc.consistency_level,
        "num_partitions":  desc.num_partitions,
        // CollectionSchema has private fields; render via Debug as a
        // diagnostic dump. Good enough for v1 since users grep it.
        "schema": format!("{:#?}", desc.schema),
    });
    let pretty = serde_json::to_string_pretty(&json)
        .map_err(|e| Error::Service(format!("describe_collection serialize: {e}")))?;
    Ok(ExecutionResult::new(
        vec!["describe".to_string()],
        vec![vec![pretty]],
        1,
    ))
}

// ----- collection_stats -------------------------------------------

pub async fn do_collection_stats(client: &Client, name: &str) -> Result<ExecutionResult> {
    let stats = client
        .get_collection_stats(name)
        .await
        .map_err(|e| svc_err("collection_stats", e))?;
    let mut rows: Vec<Vec<String>> = stats.into_iter().map(|(k, v)| vec![k, v]).collect();
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(
        vec!["field".to_string(), "value".to_string()],
        rows,
        affected,
    ))
}

// ----- list_partitions --------------------------------------------

pub async fn do_list_partitions(client: &Client, collection: &str) -> Result<ExecutionResult> {
    let parts = client
        .list_partitions(collection.to_string())
        .await
        .map_err(|e| svc_err("list_partitions", e))?;
    let rows: Vec<Vec<String>> = parts.into_iter().map(|p| vec![p]).collect();
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(
        vec!["partition".to_string()],
        rows,
        affected,
    ))
}

// ----- query (scalar filter) --------------------------------------

pub async fn do_query(
    client: &Client,
    collection: &str,
    expr: &str,
    output_fields: Vec<String>,
    partition_names: Vec<String>,
    limit: Option<i64>,
    include_vectors: bool,
) -> Result<ExecutionResult> {
    let mut opts = QueryOptions::with_output_fields(output_fields);
    if !partition_names.is_empty() {
        opts = opts.partition_names(partition_names);
    }
    if let Some(l) = limit {
        opts = opts.limit(l);
    }
    let cols = client
        .query(collection.to_string(), expr, &opts)
        .await
        .map_err(|e| svc_err("query", e))?;
    Ok(field_columns_to_result(cols, include_vectors))
}

// ----- search (vector ANN) ----------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn do_search(
    client: &Client,
    collection: &str,
    vectors: Vec<Vec<f32>>,
    metric: &str,
    limit: usize,
    output_fields: Vec<String>,
    filter: Option<String>,
    anns_field: Option<String>,
    include_vectors: bool,
) -> Result<ExecutionResult> {
    let mut opts = SearchOptions::with_limit(limit).output_fields(output_fields);
    opts = opts.add_param("metric_type", metric.to_string());
    if let Some(field) = anns_field {
        opts = opts.add_param("anns_field", field);
    }
    if let Some(f) = filter {
        opts = opts.filter(f);
    }
    let data: Vec<Value<'_>> = vectors
        .into_iter()
        .map(|v| Value::FloatArray(std::borrow::Cow::Owned(v)))
        .collect();
    let results = client
        .search(collection.to_string(), data, Some(opts))
        .await
        .map_err(|e| svc_err("search", e))?;

    // Each entry in `results` is one input vector's hit list. We flatten
    // into rows tagged by query index + rank.
    let mut columns: Vec<String> = vec![
        "query_idx".to_string(),
        "rank".to_string(),
        "score".to_string(),
    ];
    // Discover field names from the first non-empty hit's field columns.
    let mut field_names_init = false;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (qi, res) in results.iter().enumerate() {
        let scores = &res.score;
        let fields = &res.field;
        if !field_names_init {
            for f in fields {
                columns.push(f.name.clone());
            }
            field_names_init = true;
        }
        for (rank, score) in scores.iter().enumerate() {
            let mut row: Vec<String> =
                vec![qi.to_string(), rank.to_string(), format!("{score:.6}")];
            for f in fields {
                row.push(fieldcol_cell_dim(&f.value, rank, f.dim, include_vectors));
            }
            rows.push(row);
        }
    }
    let affected = rows.len() as u64;
    Ok(ExecutionResult::new(columns, rows, affected))
}

// ----- drop_collection --------------------------------------------

pub async fn do_drop_collection(client: &Client, name: &str) -> Result<ExecutionResult> {
    client
        .drop_collection(name)
        .await
        .map_err(|e| svc_err("drop_collection", e))?;
    Ok(ExecutionResult::new(
        vec!["result".to_string()],
        vec![vec![format!("dropped {name}")]],
        1,
    ))
}

// ----- load_collection --------------------------------------------

pub async fn do_load_collection(client: &Client, name: &str) -> Result<ExecutionResult> {
    client
        .load_collection(name, None)
        .await
        .map_err(|e| svc_err("load_collection", e))?;
    Ok(ExecutionResult::new(
        vec!["result".to_string()],
        vec![vec![format!("loaded {name}")]],
        1,
    ))
}

// ----- release_collection -----------------------------------------

pub async fn do_release_collection(client: &Client, name: &str) -> Result<ExecutionResult> {
    client
        .release_collection(name)
        .await
        .map_err(|e| svc_err("release_collection", e))?;
    Ok(ExecutionResult::new(
        vec!["result".to_string()],
        vec![vec![format!("released {name}")]],
        1,
    ))
}

// ----- helpers --------------------------------------------------

/// Transpose `Vec<FieldColumn>` (column-oriented) into a row-oriented
/// `ExecutionResult`. **Important**: for vector columns the inner
/// `ValueVec` is flat — `value.len() = rows * dim`. We compute the
/// row count as `value.len() / max(dim, 1)` per column and take the
/// max (scalars always have dim<=1 so this also handles them).
fn field_columns_to_result(cols: Vec<FieldColumn>, include_vectors: bool) -> ExecutionResult {
    let row_count = cols
        .iter()
        .map(|c| {
            let d = c.dim.max(1) as usize;
            c.value.len() / d
        })
        .max()
        .unwrap_or(0);
    let columns: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(row_count);
    for i in 0..row_count {
        let row: Vec<String> = cols
            .iter()
            .map(|c| fieldcol_cell_dim(&c.value, i, c.dim, include_vectors))
            .collect();
        rows.push(row);
    }
    ExecutionResult::new(columns, rows, row_count as u64)
}

/// Stringify one cell (row index `i`) from a `ValueVec`, honoring the
/// column's `dim`. When `dim > 1` (vector column) and `include_vectors`
/// is false, emit a `<vec dim=D>` placeholder — saves a lot of tokens
/// since users rarely need the raw floats inline.
fn fieldcol_cell_dim(v: &ValueVec, i: usize, dim: i64, include_vectors: bool) -> String {
    let d = dim.max(1) as usize;
    if d > 1 {
        // Vector column.
        if !include_vectors {
            return format!("<vec dim={d}>");
        }
        let start = i * d;
        let end = start + d;
        match v {
            ValueVec::Float(xs) if end <= xs.len() => {
                let slice: Vec<String> = xs[start..end].iter().map(|x| format!("{x:.6}")).collect();
                format!("[{}]", slice.join(","))
            }
            ValueVec::Double(xs) if end <= xs.len() => {
                let slice: Vec<String> = xs[start..end].iter().map(|x| format!("{x:.6}")).collect();
                format!("[{}]", slice.join(","))
            }
            // BinaryVec / FloatVec is sometimes packed as Vec<u8>; render
            // the byte slice length.
            ValueVec::Binary(xs) if end <= xs.len() => format!("<{} bytes>", end - start),
            _ => "<vec ?>".to_string(),
        }
    } else {
        fieldcol_cell(v, i)
    }
}

fn fieldcol_cell(v: &ValueVec, i: usize) -> String {
    match v {
        ValueVec::None => String::new(),
        ValueVec::Bool(xs) => xs.get(i).map(|x| x.to_string()).unwrap_or_default(),
        ValueVec::Int(xs) => xs.get(i).map(|x| x.to_string()).unwrap_or_default(),
        ValueVec::Long(xs) => xs.get(i).map(|x| x.to_string()).unwrap_or_default(),
        ValueVec::Float(xs) => xs.get(i).map(|x| format!("{x:.6}")).unwrap_or_default(),
        ValueVec::Double(xs) => xs.get(i).map(|x| format!("{x:.6}")).unwrap_or_default(),
        ValueVec::Binary(xs) => {
            if xs.is_empty() {
                String::new()
            } else {
                format!("<{} bytes>", xs.len())
            }
        }
        ValueVec::String(xs) => xs.get(i).cloned().unwrap_or_default(),
        ValueVec::Json(xs) => xs
            .get(i)
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s.to_string())
            .unwrap_or_default(),
        ValueVec::Array(_) => "<array>".to_string(),
        ValueVec::Bytes(xs) => xs
            .get(i)
            .map(|b| format!("<{} bytes>", b.len()))
            .unwrap_or_default(),
        ValueVec::Geometry(_) => "<geometry>".to_string(),
    }
}
