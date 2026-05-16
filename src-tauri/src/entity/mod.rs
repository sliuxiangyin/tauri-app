//! SeaORM 实体模块；表结构确定后在此目录下按表拆分文件并在 `prelude` 中 re-export。

pub mod mcp_serve_config;
pub mod model_provider_config;
pub mod model_provider_model;

pub mod prelude {
    pub use super::mcp_serve_config::{self as msc, Entity as McpServeConfigEntity};
    pub use super::model_provider_config::{self as mpc, Entity as ModelProviderConfigEntity};
    pub use super::model_provider_model::{self as mpm, Entity as ModelProviderModelEntity};
}
