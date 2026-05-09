//! PostgreSQL connection + executor primitives, layered on `tools4a-core`.

pub mod connection;
pub mod execute;
pub mod executor;

pub use connection::PgsqlConnection;
pub use execute::{PgsqlParams, execute};
pub use executor::PgsqlExecutor;
