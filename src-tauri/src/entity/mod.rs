//! SeaORM 实体模块；表结构确定后在此目录下按表拆分文件并在 `prelude` 中 re-export。

pub mod chat_message;
pub mod mcp_serve_config;
pub mod model_provider_config;
pub mod model_provider_model;

// 导出各实体的 Entity 和 Model
pub use chat_message::{Entity as ChatMessageEntity, Model as ChatMessageModel};
pub use mcp_serve_config::{Entity as McpServeConfigEntity, Model as McpServeConfigModel};
pub use model_provider_config::{
    Entity as ModelProviderConfigEntity, Model as ModelProviderConfigModel,
};
pub use model_provider_model::{
    Entity as ModelProviderModelEntity, Model as ModelProviderModelModel,
};
