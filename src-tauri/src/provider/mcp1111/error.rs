use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("MCP connection error: {0}")]
    ConnectionError(String),

    #[error("MCP protocol error: {0}")]
    ProtocolError(String),

    #[error("MCP communication error: {0}")]
    CommunicationError(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool execution error: {0}")]
    ToolExecutionError(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Service not connected: {0}")]
    ServiceNotConnected(String),

    #[error("Invalid transport: {0}")]
    InvalidTransport(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection failed after 3 retries")]
    ConnectionFailedAfterRetries,

    #[error("Service not found: {0}")]
    ServiceNotFound(String),
}

impl From<McpError> for String {
    fn from(err: McpError) -> Self {
        err.to_string()
    }
}
