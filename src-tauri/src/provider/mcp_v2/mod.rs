pub mod api;
pub mod config;
pub mod connection;
pub mod error;
pub mod server_manager;
pub mod transport;

pub use api::McpV2Api;

/// MCP v2 异步初始化占位类型
/// 同步注册空壳，异步任务完成后填充；命令调用时检查是否就绪
pub type McpV2State = std::sync::Arc<tokio::sync::RwLock<Option<McpV2Api>>>;
pub use server_manager::{ServerManager, ToolWithSource};
