//! Chat Model 服务相关类型定义
//!
//! 包含：
//! - 服务内部使用：AccountModelSelection
//! - 前端 DTO：ModelItem, ModelGroup

use serde::{Deserialize, Serialize};

/// 账户模型选择配置（服务内部使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountModelSelection {
    pub config_id: String,
    pub model_id: String,
}

/// 模型项（前端 DTO）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelItem {
    pub model_id: String,
    pub model_name: String,
}

/// 分组后的模型配置（前端 DTO）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGroup {
    pub name: String,
    pub id: String,
    pub items: Vec<ModelItem>,
}