use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpManagerError {
    #[error("MCP server '{server_id}' not found")]
    ServerNotFound { server_id: String },

    #[error("MCP server '{server_id}' already exists")]
    ServerAlreadyExists { server_id: String },

    #[error("MCP connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("MCP tool call failed: {message}")]
    ToolCallFailed { message: String },

    #[error("MCP tool '{tool_name}' not found on server '{server_id}'")]
    ToolNotFound {
        server_id: String,
        tool_name: String,
    },

    #[error("Cache operation failed: {0}")]
    CacheError(String),

    #[error("Serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("MCP protocol error: {0}")]
    McpError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Transport build failed: {message}")]
    TransportError { message: String },

    #[error("Internal error: {message}")]
    Internal { message: String },
}

pub type Result<T> = std::result::Result<T, McpManagerError>;

impl From<McpManagerError> for String {
    fn from(err: McpManagerError) -> Self {
        err.to_string()
    }
}
