//! 服务层 - 跨模块业务编排与协调
//!
//! 职责边界：
//! - ✅ 跨 Provider 协作
//! - ✅ 业务编排与初始化
//! - ✅ 调用 services::db 获取数据（不直接访问数据库）
//! - ❌ 不直接调用外部 HTTP API（由 Provider 层负责）
//!
//! ## Trait 接口
//!
//! Services 层通过 Trait 接口解耦具体实现：
//! - `DbAccessor`: 数据库访问
//! - `CacheAccessor`: 缓存访问
//! - `McpClient`: MCP 客户端

pub mod chat_model_service;
pub mod chat_tools_service;
pub mod db;
pub mod llm_service;
pub mod llm_service_test;
pub mod mcp_service;
pub mod messages;
pub mod messages_service;
pub mod llm;
pub mod wechat_message;
pub mod traits; // Trait 接口定义（依赖注入核心）

// Re-export traits for convenience
pub use traits::{DbAccessor, McpClient};

// Re-export commonly used functions
pub use chat_model_service::get_provider_config;
