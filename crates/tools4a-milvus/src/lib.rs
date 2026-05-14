//! Milvus vector database leaf crate. Wraps the BenLocal/milvus-sdk-rust
//! fork (branch `self`). Ten MCP tools — 6 read, 1 vector search, 3
//! write (allow_write-gated).
//!
//! See `docs/superpowers/plans/2026-05-14-tools-mcp-phase18-milvus.md`.

pub mod actions;
pub mod connection;
pub mod mcp;
pub mod orchestrator;
pub mod run;

pub use actions::MilvusAction;
pub use connection::connect_milvus;
pub use mcp::{
    MilvusCollectionStatsMcp, MilvusCollectionStatsParams, MilvusConnectionFields,
    MilvusDescribeCollectionMcp, MilvusDescribeCollectionParams, MilvusDropCollectionMcp,
    MilvusDropCollectionParams, MilvusListCollectionsMcp, MilvusListCollectionsParams,
    MilvusListDatabasesMcp, MilvusListDatabasesParams, MilvusListPartitionsMcp,
    MilvusListPartitionsParams, MilvusLoadCollectionMcp, MilvusLoadCollectionParams,
    MilvusQueryMcp, MilvusQueryParams, MilvusReleaseCollectionMcp, MilvusReleaseCollectionParams,
    MilvusSearchMcp, MilvusSearchParams,
};
pub use orchestrator::{MilvusOrchestrator, MilvusRequest};
pub use run::run;
