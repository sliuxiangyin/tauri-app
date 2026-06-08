//! SeaORM 实体模块；表结构确定后在此目录下按表拆分文件并在 `prelude` 中 re-export。

pub mod chat_message;
pub mod conversations;
pub mod mcp_model;
pub mod message;
pub mod model_provider_config;
pub mod model_provider_model;
pub mod plans;
pub mod tool_call;

#[allow(unused_imports)]
pub use chat_message::Entity as ChatMessageEntity;
pub use conversations::Entity as ConversationEntity;
pub use mcp_model::Entity as McpEntity;
pub use message::Entity as MessageEntity;
pub use plans::Entity as PlanEntity;
#[allow(unused_imports)]
pub use tool_call::Entity as ToolCallEntity;
