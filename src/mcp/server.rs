use crate::error::{Error, Result};

/// Run the MCP server over stdio. Blocks until the client disconnects.
pub async fn serve_stdio() -> Result<()> {
    Err(Error::Connection(
        "MCP server not yet implemented".to_string(),
    ))
}
