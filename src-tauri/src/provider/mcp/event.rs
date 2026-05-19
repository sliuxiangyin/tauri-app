use serde::{Deserialize, Serialize};

/// MCP 事件类型
/// 用于通知外部 MCP 服务器状态变化
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum McpEvent {
    /// 服务器已入队列，等待处理
    ServerPending { server_id: String, name: String },
    /// 服务器正在安装（仅 STDIO 类型）
    ServerInstalling { server_id: String, name: String, progress: u8 },
    /// 服务器正在连接
    ServerConnecting { server_id: String, name: String },
    /// 服务器连接成功
    ServerConnected { server_id: String, name: String, tool_count: usize },
    /// 服务器连接失败
    ServerFailed { server_id: String, name: String, error: String },
    /// 服务器连接断开（外部进程被终止）
    ServerDisconnected { server_id: String, name: String, reason: String },
    /// 服务器已停止
    ServerStopped { server_id: String, name: String },
    /// 服务器已删除
    ServerRemoved { server_id: String, name: String },
}

impl McpEvent {
    /// 获取服务器 ID
    pub fn server_id(&self) -> &str {
        match self {
            McpEvent::ServerPending { server_id, .. } => server_id,
            McpEvent::ServerInstalling { server_id, .. } => server_id,
            McpEvent::ServerConnecting { server_id, .. } => server_id,
            McpEvent::ServerConnected { server_id, .. } => server_id,
            McpEvent::ServerFailed { server_id, .. } => server_id,
            McpEvent::ServerDisconnected { server_id, .. } => server_id,
            McpEvent::ServerStopped { server_id, .. } => server_id,
            McpEvent::ServerRemoved { server_id, .. } => server_id,
        }
    }

    /// 获取服务器名称
    pub fn name(&self) -> &str {
        match self {
            McpEvent::ServerPending { name, .. } => name,
            McpEvent::ServerInstalling { name, .. } => name,
            McpEvent::ServerConnecting { name, .. } => name,
            McpEvent::ServerConnected { name, .. } => name,
            McpEvent::ServerFailed { name, .. } => name,
            McpEvent::ServerDisconnected { name, .. } => name,
            McpEvent::ServerStopped { name, .. } => name,
            McpEvent::ServerRemoved { name, .. } => name,
        }
    }
}

/// MCP 事件发送器类型别名
/// 由外部创建并传入 MCP 模块，用于异步发送事件
pub type McpEventSender = tokio::sync::mpsc::UnboundedSender<McpEvent>;

/// MCP 事件接收器类型别名
pub type McpEventReceiver = tokio::sync::mpsc::UnboundedReceiver<McpEvent>;

/// 创建新的事件 channel
pub fn create_event_channel() -> (McpEventSender, McpEventReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}