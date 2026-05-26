//! MCP 模块错误类型定义

use rmcp::service::ServiceError;

/// MCP 运行时错误
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("connection failed for '{name}': {source}")]
    Connection {
        name: String,
        source: Box<McpError>,
    },

    #[error("service error: {0}")]
    Service(#[from] ServiceError),

    #[error("MCP server '{0}' not found in pool")]
    NotFound(String),

    #[error("MCP server '{name}' is already connected")]
    AlreadyConnected { name: String },

    #[error("MCP server '{name}' circuit breaker is open")]
    CircuitOpen { name: String },

    #[error("MCP server '{name}' is not connected")]
    NotConnected { name: String },

    #[error("tool call timeout for '{name}'")]
    ToolCallTimeout { name: String },

    #[error("connection closed for '{name}': {reason}")]
    ConnectionClosed { name: String, reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl McpError {
    /// 判断是否为可恢复的连接错误（触发重连）
    pub fn is_connection_error(&self) -> bool {
        matches!(
            self,
            McpError::ConnectionClosed { .. }
                | McpError::Service(_)
                | McpError::Transport(_)
                | McpError::Io(_)
        )
    }

    /// 判断是否为熔断器打开（不可恢复，等待冷却）
    pub fn is_circuit_open(&self) -> bool {
        matches!(self, McpError::CircuitOpen { .. })
    }

    /// 提取关联的 MCP 服务名称
    pub fn server_name(&self) -> Option<&str> {
        match self {
            McpError::NotFound(name)
            | McpError::AlreadyConnected { name }
            | McpError::CircuitOpen { name }
            | McpError::NotConnected { name }
            | McpError::ToolCallTimeout { name }
            | McpError::ConnectionClosed { name, .. } => Some(name),
            McpError::Connection { name, .. } => Some(name),
            _ => None,
        }
    }
}

/// MCP 模块 Result 别名
pub type McpResult<T> = Result<T, McpError>;
