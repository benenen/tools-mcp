//! Dispatcher — switches on `MilvusAction` and calls the matching
//! action function.

use crate::actions::{self, MilvusAction};
use milvus::client::Client;
use tools4a_core::{ExecutionResult, Result};

pub async fn run(client: &Client, action: MilvusAction) -> Result<ExecutionResult> {
    match action {
        MilvusAction::ListDatabases => actions::do_list_databases(client).await,
        MilvusAction::ListCollections => actions::do_list_collections(client).await,
        MilvusAction::DescribeCollection { name } => {
            actions::do_describe_collection(client, &name).await
        }
        MilvusAction::CollectionStats { name } => actions::do_collection_stats(client, &name).await,
        MilvusAction::ListPartitions { collection } => {
            actions::do_list_partitions(client, &collection).await
        }
        MilvusAction::Query {
            collection,
            expr,
            output_fields,
            partition_names,
            limit,
            include_vectors,
        } => {
            actions::do_query(
                client,
                &collection,
                &expr,
                output_fields,
                partition_names,
                limit,
                include_vectors,
            )
            .await
        }
        MilvusAction::Search {
            collection,
            vectors,
            metric,
            limit,
            output_fields,
            filter,
            anns_field,
            include_vectors,
        } => {
            actions::do_search(
                client,
                &collection,
                vectors,
                &metric,
                limit,
                output_fields,
                filter,
                anns_field,
                include_vectors,
            )
            .await
        }
        MilvusAction::DropCollection { name } => actions::do_drop_collection(client, &name).await,
        MilvusAction::LoadCollection { name } => actions::do_load_collection(client, &name).await,
        MilvusAction::ReleaseCollection { name } => {
            actions::do_release_collection(client, &name).await
        }
    }
}
