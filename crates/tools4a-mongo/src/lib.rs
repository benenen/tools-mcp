//! MongoDB connection + executor primitives, layered on `tools4a-core`.
//!
//! Commands are JSON documents passed to `Database::run_command`. The
//! returned BSON Document is serialized back to JSON and presented as a
//! single `result` row, matching the redis_exec mapping convention for
//! non-tabular results.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::MongoConnection;
pub use execute::{MongoParams, execute};
pub use executor::MongoExecutor;
