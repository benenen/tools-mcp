//! Ten `McpTool` impls for the Milvus leaf. All share `MilvusConnectionFields`
//! (host/port/scheme/user/password + tunnel + timeout) via `#[serde(flatten)]`.

use crate::actions::MilvusAction;
use crate::orchestrator::{DEFAULT_PORT, MilvusOrchestrator, MilvusRequest};
use async_trait::async_trait;

use schemars::JsonSchema;
use serde::Deserialize;
use tools4a_core::{
    ExecutionResult, McpTool, Result, Service, SshJumpInput, TunnelKind, build_tunnel_config,
};

// -- Shared connection fields ----------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct MilvusConnectionFields {
    /// Milvus host (required).
    pub host: String,
    /// "http" (default) or "https".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// Default 19530.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Basic auth user (Milvus 2.x).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_jump: Option<SshJumpInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

fn build_req(
    conn: MilvusConnectionFields,
    action: MilvusAction,
    allow_write: bool,
) -> Result<(MilvusRequest, Option<tools4a_core::TunnelConfig>)> {
    let scheme = conn.scheme.unwrap_or_else(|| "http".to_string());
    let port = conn.port.unwrap_or(DEFAULT_PORT);
    let tunnel = build_tunnel_config(
        conn.tunnel,
        conn.ssh_jump,
        conn.ssh_user,
        conn.ssh_password,
        conn.ssh_key_path,
        conn.ssh_port,
    )?;
    let req = MilvusRequest {
        action,
        scheme,
        host: conn.host,
        port,
        username: conn.user,
        password: conn.password,
        allow_write,
        timeout_secs: conn.timeout_secs,
        max_timeout_secs: None,
    };
    Ok((req, tunnel))
}

// -- milvus_list_databases ------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusListDatabasesParams {
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusListDatabasesMcp;
#[async_trait]
impl McpTool for MilvusListDatabasesMcp {
    const NAME: &'static str = "milvus_list_databases";
    const DESCRIPTION: &'static str = "List Milvus databases. Read-only.";
    type Params = MilvusListDatabasesParams;

    async fn invoke(p: MilvusListDatabasesParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(p.conn, MilvusAction::ListDatabases, false)?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_list_collections ----------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusListCollectionsParams {
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusListCollectionsMcp;
#[async_trait]
impl McpTool for MilvusListCollectionsMcp {
    const NAME: &'static str = "milvus_list_collections";
    const DESCRIPTION: &'static str = "List collections in the current Milvus database. Read-only.";
    type Params = MilvusListCollectionsParams;

    async fn invoke(p: MilvusListCollectionsParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(p.conn, MilvusAction::ListCollections, false)?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_describe_collection -------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusDescribeCollectionParams {
    pub name: String,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusDescribeCollectionMcp;
#[async_trait]
impl McpTool for MilvusDescribeCollectionMcp {
    const NAME: &'static str = "milvus_describe_collection";
    const DESCRIPTION: &'static str =
        "Inspect a Milvus collection: schema, shards, aliases, partitions count. Read-only.";
    type Params = MilvusDescribeCollectionParams;

    async fn invoke(p: MilvusDescribeCollectionParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::DescribeCollection { name: p.name },
            false,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_collection_stats ----------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusCollectionStatsParams {
    pub name: String,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusCollectionStatsMcp;
#[async_trait]
impl McpTool for MilvusCollectionStatsMcp {
    const NAME: &'static str = "milvus_collection_stats";
    const DESCRIPTION: &'static str =
        "Get collection statistics (row_count etc.) as field/value pairs. Read-only.";
    type Params = MilvusCollectionStatsParams;

    async fn invoke(p: MilvusCollectionStatsParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::CollectionStats { name: p.name },
            false,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_list_partitions -----------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusListPartitionsParams {
    pub collection: String,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusListPartitionsMcp;
#[async_trait]
impl McpTool for MilvusListPartitionsMcp {
    const NAME: &'static str = "milvus_list_partitions";
    const DESCRIPTION: &'static str = "List partitions of a Milvus collection. Read-only.";
    type Params = MilvusListPartitionsParams;

    async fn invoke(p: MilvusListPartitionsParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::ListPartitions {
                collection: p.collection,
            },
            false,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_query (scalar filter) -----------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusQueryParams {
    pub collection: String,
    /// Filter expression. e.g. `id > 100 and category == "foo"`.
    pub expr: String,
    /// Output fields. Empty = SDK default (primary key + dynamic fields).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partition_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    /// When true, return raw float arrays for vector fields. Default
    /// false — vector cells render as `<vec dim=N>` placeholders to
    /// save tokens. Set true only when you really need the floats.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_vectors: bool,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusQueryMcp;
#[async_trait]
impl McpTool for MilvusQueryMcp {
    const NAME: &'static str = "milvus_query";
    const DESCRIPTION: &'static str = "Query a Milvus collection by scalar filter expression. Returns rows of the requested \
         output_fields. Vector fields are elided by default (set include_vectors=true to get raw floats). Read-only.";
    type Params = MilvusQueryParams;

    async fn invoke(p: MilvusQueryParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::Query {
                collection: p.collection,
                expr: p.expr,
                output_fields: p.output_fields,
                partition_names: p.partition_names,
                limit: p.limit,
                include_vectors: p.include_vectors,
            },
            false,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_search (vector ANN) -------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusSearchParams {
    pub collection: String,
    /// Vector data as a 2D array of f32 (one inner array per query).
    pub vectors: Vec<Vec<f32>>,
    /// Distance metric. Common: `L2`, `IP`, `COSINE`. Default `L2`.
    #[serde(default = "default_metric")]
    pub metric: String,
    /// Top-K per query vector. Default 10.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Output fields (in addition to score + ids).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_fields: Vec<String>,
    /// Optional pre-filter (scalar expression).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Explicit `anns_field` (vector field name). Required when the
    /// collection has multiple vector fields; auto-picked otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anns_field: Option<String>,
    /// See `MilvusQueryParams.include_vectors`. Default false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_vectors: bool,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

fn default_metric() -> String {
    "L2".to_string()
}
fn default_limit() -> usize {
    10
}

pub struct MilvusSearchMcp;
#[async_trait]
impl McpTool for MilvusSearchMcp {
    const NAME: &'static str = "milvus_search";
    const DESCRIPTION: &'static str = "Vector ANN search in a Milvus collection. Pass `vectors` as a 2D array of f32 (one inner \
         array per query). Returns rows tagged by query_idx + rank + score + requested fields.";
    type Params = MilvusSearchParams;

    async fn invoke(p: MilvusSearchParams) -> Result<ExecutionResult> {
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::Search {
                collection: p.collection,
                vectors: p.vectors,
                metric: p.metric,
                limit: p.limit,
                output_fields: p.output_fields,
                filter: p.filter,
                anns_field: p.anns_field,
                include_vectors: p.include_vectors,
            },
            false,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_drop_collection (write) ---------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusDropCollectionParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusDropCollectionMcp;
#[async_trait]
impl McpTool for MilvusDropCollectionMcp {
    const NAME: &'static str = "milvus_drop_collection";
    const DESCRIPTION: &'static str =
        "Drop a Milvus collection (destructive). Requires allow_write=true.";
    type Params = MilvusDropCollectionParams;

    async fn invoke(p: MilvusDropCollectionParams) -> Result<ExecutionResult> {
        let allow_write = p.allow_write;
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::DropCollection { name: p.name },
            allow_write,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_load_collection -----------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusLoadCollectionParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusLoadCollectionMcp;
#[async_trait]
impl McpTool for MilvusLoadCollectionMcp {
    const NAME: &'static str = "milvus_load_collection";
    const DESCRIPTION: &'static str = "Load a Milvus collection into memory (required before query/search). \
         Requires allow_write=true since it changes cluster state.";
    type Params = MilvusLoadCollectionParams;

    async fn invoke(p: MilvusLoadCollectionParams) -> Result<ExecutionResult> {
        let allow_write = p.allow_write;
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::LoadCollection { name: p.name },
            allow_write,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}

// -- milvus_release_collection --------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MilvusReleaseCollectionParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_write: bool,
    #[serde(flatten)]
    pub conn: MilvusConnectionFields,
}

pub struct MilvusReleaseCollectionMcp;
#[async_trait]
impl McpTool for MilvusReleaseCollectionMcp {
    const NAME: &'static str = "milvus_release_collection";
    const DESCRIPTION: &'static str = "Release a Milvus collection from memory. Requires allow_write=true since it changes \
         cluster state.";
    type Params = MilvusReleaseCollectionParams;

    async fn invoke(p: MilvusReleaseCollectionParams) -> Result<ExecutionResult> {
        let allow_write = p.allow_write;
        let (req, tunnel) = build_req(
            p.conn,
            MilvusAction::ReleaseCollection { name: p.name },
            allow_write,
        )?;
        MilvusOrchestrator::execute(req, tunnel).await
    }
}
