//! 服务层 - 跨模块业务编排与协调
//!
//! 职责边界：
//! - ✅ 跨 Provider 协作
//! - ✅ 业务编排与初始化
//! - ✅ 调用 services::db 获取数据（不直接访问数据库）
//! - ❌ 不直接调用外部 HTTP API（由 Provider 层负责）

pub mod chat_model_service;
pub mod chat_tools_service;
pub mod db;
pub mod llm_service;
pub mod mcp_service;
pub mod wechat_message;
