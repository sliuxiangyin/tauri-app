//! SeaORM 实体模块；表结构确定后在此目录下按表拆分文件并在 `prelude` 中 re-export。

pub mod chat_message;
pub mod mcp_model;
pub mod mcp_serve_config;
pub mod model_provider_config;
pub mod model_provider_model;

pub use chat_message::Entity as ChatMessageEntity;
pub use mcp_model::Entity as McpEntity;
