//! MCP 服务管理模块
//! 
//! 核心约束：
//! - 禁止在 mcp 模块中直接引入 tauri::AppHandle
//! - 事件通知通过 mpsc::Sender 异步转发，外部消费并转发给 Tauri
//! - 所有数据库操作必须在 mcp 模块外部进行

pub mod config;
pub mod connection;
pub mod error;
pub mod event;
pub mod manager;
pub mod process;
pub mod queue;
pub mod state;

// 导出公共类型
pub use config::{McpServerConfig, TransportConfig};
pub use error::{is_connection_error, McpError, Result};
pub use event::{create_event_channel, McpEvent, McpEventReceiver, McpEventSender};
pub use manager::{ServerInfo, ServerState, ToolWithSource};
pub use queue::{McpQueue, QueueItem};
pub use state::McpState;

/// 默认最大并发数
pub const DEFAULT_MAX_CONCURRENCY: usize = 3;