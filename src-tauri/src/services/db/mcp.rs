//! MCP 模型数据库服务层
//! 提供 MCP 配置的 CRUD 操作接口，name 作为唯一标识

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::entity::mcp_model::{self, ActiveModel, Model as McpModel};
use crate::entity::McpEntity;

/// MCP DTO（返回给前端）
#[derive(Debug, Clone, serde::Serialize)]
pub struct McpDto {
    pub id: i32,
    pub name: String,
    pub transport: String,
    pub config: String,
    pub status: String,
    pub operating: String,
    pub tools: Option<String>,
    pub error_msg: Option<String>,
    pub updated_at: String,
}

impl From<McpModel> for McpDto {
    fn from(model: McpModel) -> Self {
        Self {
            id: model.id,
            name: model.name,
            transport: model.transport,
            config: model.config,
            status: model.status,
            operating: model.operating,
            tools: if model.tools.is_empty() { None } else { Some(model.tools) },
            error_msg: if model.error_msg.is_empty() { None } else { Some(model.error_msg) },
            updated_at: model.updated_at.to_string(),
        }
    }
}

/// 创建 MCP Payload
/// config 直接接收 JSON 字符串，由 Command 层解析 transport
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateMcpPayload {
    pub name: String,
    pub transport: String,
    pub config: String,  // JSON 字符串直接存储
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "enable".to_string()
}

/// 更新 MCP Payload（不包括 name）
/// config 为 JSON 字符串
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct UpdateMcpPayload {
    pub transport: Option<String>,
    pub config: Option<String>,  // JSON 字符串
    pub status: Option<String>,
    pub tools: Option<String>,
    pub error_msg: Option<String>,
    pub operating: Option<String>,
}

/// 获取所有 MCP 配置
pub async fn get_all_mcps(db: &DatabaseConnection) -> Result<Vec<McpDto>, String> {
    let models = McpEntity::find()
        .order_by_asc(mcp_model::Column::Id)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(models.into_iter().map(McpDto::from).collect())
}

/// 通过 name 获取单个 MCP 配置
pub async fn get_mcp_by_name(db: &DatabaseConnection, name: &str) -> Result<Option<McpDto>, String> {
    let model = McpEntity::find()
        .filter(mcp_model::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(model.map(McpDto::from))
}

/// 创建 MCP 配置（name 唯一）
pub async fn create_mcp(
    db: &DatabaseConnection,
    payload: CreateMcpPayload,
) -> Result<McpDto, String> {
    // 检查 name 是否已存在
    let existing = McpEntity::find()
        .filter(mcp_model::Column::Name.eq(&payload.name))
        .one(db)
        .await
        .map_err(|e| e.to_string())?;

    if existing.is_some() {
        return Err(format!("MCP with name '{}' already exists", payload.name));
    }

    let now = chrono::Utc::now().timestamp();
    // config 已经是 JSON 字符串，直接存储
    let config_json = payload.config;

    let active_model = ActiveModel {
        name: Set(payload.name),
        transport: Set(payload.transport),
        config: Set(config_json),
        status: Set(payload.status),
        operating: Set(mcp_model::OPERATING_IDLE.to_string()),
        tools: Set(String::new()),
        error_msg: Set(String::new()),
        updated_at: Set(now),
        ..Default::default()
    };

    let model = active_model.insert(db).await.map_err(|e| e.to_string())?;
    Ok(McpDto::from(model))
}

/// 通过 name 更新 MCP 配置（不允许修改 name）
pub async fn update_mcp_by_name(
    db: &DatabaseConnection,
    name: &str,
    payload: UpdateMcpPayload,
) -> Result<McpDto, String> {
    let model = McpEntity::find()
        .filter(mcp_model::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("MCP not found: {}", name))?;

    let mut active_model: ActiveModel = model.into();

    if let Some(transport) = payload.transport {
        active_model.transport = Set(transport);
    }
    if let Some(config) = payload.config {
        // config 已经是 JSON 字符串，直接存储
        active_model.config = Set(config);
    }
    if let Some(status) = payload.status {
        active_model.status = Set(status);
    }
    if let Some(tools) = payload.tools {
        active_model.tools = Set(tools);
    }
    if let Some(error_msg) = payload.error_msg {
        active_model.error_msg = Set(error_msg);
    }
    if let Some(operating) = payload.operating {
        active_model.operating = Set(operating);
    }

    // 更新 updated_at
    active_model.updated_at = Set(chrono::Utc::now().timestamp());

    let model = active_model.update(db).await.map_err(|e| e.to_string())?;
    Ok(McpDto::from(model))
}

/// 通过 name 删除 MCP 配置（硬删除）
pub async fn delete_mcp_by_name(db: &DatabaseConnection, name: &str) -> Result<(), String> {
    let model = McpEntity::find()
        .filter(mcp_model::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("MCP not found: {}", name))?;

    let active_model: ActiveModel = model.into();
    active_model.delete(db).await.map_err(|e| e.to_string())?;

    tracing::info!("Hard deleted MCP: {}", name);
    Ok(())
}

/// 更新运行状态和错误信息
pub async fn update_mcp_operating_and_error(
    db: &DatabaseConnection,
    name: &str,
    operating: &str,
    error_msg: Option<&str>,
) -> Result<(), String> {
    let model = McpEntity::find()
        .filter(mcp_model::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("MCP not found: {}", name))?;

    let mut active_model: ActiveModel = model.into();
    active_model.operating = Set(operating.to_string());
    active_model.error_msg = Set(error_msg.unwrap_or("").to_string());
    active_model.updated_at = Set(chrono::Utc::now().timestamp());

    active_model.update(db).await.map_err(|e| e.to_string())?;
    Ok(())
}

