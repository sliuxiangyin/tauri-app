use thiserror::Error;

/// MCP 管理器错误类型
#[derive(Debug, Error)]
pub enum McpError {
    #[error("MCP server '{server_id}' not found")]
    ServerNotFound { server_id: String },

    #[error("MCP server '{server_id}' already exists")]
    ServerAlreadyExists { server_id: String },

    #[error("MCP connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("MCP connection lost: {message}")]
    ConnectionLost { server_id: String, message: String },

    #[error("MCP tool call failed: {message}")]
    ToolCallFailed { message: String },

    #[error("MCP tool '{tool_name}' not found on server '{server_id}'")]
    ToolNotFound {
        server_id: String,
        tool_name: String,
    },

    #[error("Transport build failed: {message}")]
    TransportError { message: String },

    #[error("Process error: {message}")]
    ProcessError { message: String },

    #[error("Queue error: {message}")]
    QueueError { message: String },

    #[error("Serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// MCP 操作结果类型
pub type Result<T> = std::result::Result<T, McpError>;

/// 判断是否为连接错误（用于自动重连判断）
pub fn is_connection_error(err: &McpError) -> bool {
    matches!(
        err,
        McpError::ConnectionLost { .. }
            | McpError::ConnectionFailed { .. }
            | McpError::Io(_)
    )
}