use sea_orm::{ActiveModelTrait, EntityTrait, NotSet, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use crate::db::DbState;
use crate::entity::mcp_serve_config::{self as msc, McpModelConfig};
use crate::provider::mcp_v2::{McpServerConfig, McpV2State, ToolWithSource};
// ---------------------------------------------------------------------------
// Config 结构化 DTO（与前端 mcp-service.ts 对应）
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServeConfigDto {
    pub id: i32,
    pub name: String,
    pub config: McpModelConfig,
    pub state: bool,
    pub tools: Vec<ToolWithSource>,
    //失败原因
    pub error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMcpServeConfigPayload {
    pub name: String,
    pub config: McpModelConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateMcpServeConfigPayload {
    pub name: Option<String>,
    pub config: Option<McpModelConfig>,
}

fn model_to_dto(m: msc::Model) -> Result<McpServeConfigDto, String> {
    let config: McpModelConfig =
        serde_json::from_str(&m.config).map_err(|e| format!("config parse error: {e}"))?;
    Ok(McpServeConfigDto {
        id: m.id,
        name: m.name,
        config,
        state: false,
        tools: Vec::new(),
        error: None,
        updated_at: m.updated_at.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Tauri 命令
// ---------------------------------------------------------------------------

/// 列出所有 MCP 服务配置（含运行时状态）
#[tauri::command]
pub async fn list_mcp_serve_configs(
    state: tauri::State<'_, DbState>,
    mcp_state: tauri::State<'_, McpV2State>,
) -> Result<Vec<McpServeConfigDto>, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let configs = msc::Entity::find()
        .order_by_asc(msc::Column::Id)
        .all(&*db)
        .await
        .map_err(|e| e.to_string())?;

    let mut dtos: Vec<McpServeConfigDto> = configs
        .into_iter()
        .map(model_to_dto)
        .collect::<Result<Vec<_>, _>>()?;

    // MCP v2 可能尚未初始化完成，读取时检查
    let guard = mcp_state.read().await;
    if let Some(ref mcp_api) = *guard {
        let servers = mcp_api.list_tools(None).await?;
        // println!("servers11: {:?}", servers);
        // 获取运行时状态并合并
        // let states = mcp_api.().await;
        // for dto in &mut dtos {
        //     let service_id_str = dto.id.to_string();
        //     if let Some(st) = states.iter().find(|s| s.id == service_id_str) {
        //         dto.state = st.state;
        //         dto.tools = st.tools.clone();
        //         dto.error = st.error.clone();
        //     }
        // }
    }

    Ok(dtos)
}

/// 创建新的 MCP 服务配置
#[tauri::command]
pub async fn create_mcp_serve_config(
    state: tauri::State<'_, DbState>,
    payload: CreateMcpServeConfigPayload,
    mcp_state: tauri::State<'_, McpV2State>,
) -> Result<McpServeConfigDto, String> {
    let db: std::sync::Arc<sea_orm::prelude::DatabaseConnection> = state.get().await.map_err(|e| e.to_string())?;

    let config_json = serde_json::to_string(&payload.config)
        .map_err(|e| format!("config serialize error: {e}"))?;

    let active = msc::ActiveModel {
        id: NotSet,
        name: Set(payload.name),
        config: Set(config_json),
        updated_at: Set(chrono::Utc::now()),
    };

    let model = active.insert(&*db).await.map_err(|e| e.to_string())?;
    let guard = mcp_state.read().await;
    if let Some(ref mcp_api) = *guard {
        match McpServerConfig::try_from(model.clone()) {
            Ok(config) => {
                mcp_api.add_server(config).await?;
            }
            Err(e) => {
                tracing::warn!("新增 MCP 服务配置时转换失败: {}", e);
            }
        }
    }
    model_to_dto(model)
}

/// 更新已有配置
#[tauri::command]
pub async fn update_mcp_serve_config(
    state: tauri::State<'_, DbState>,
    id: i32,
    payload: UpdateMcpServeConfigPayload,
    mcp_state: tauri::State<'_, McpV2State>,
) -> Result<McpServeConfigDto, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    let entity = msc::Entity::find_by_id(id)
        .one(&*db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("mcp serve config not found: {id}"))?;

    let mut active: msc::ActiveModel = entity.into();

    if let Some(v) = payload.name {
        active.name = Set(v);
    }
    if let Some(config) = payload.config {
        let config_json =
            serde_json::to_string(&config).map_err(|e| format!("config serialize error: {e}"))?;
        active.config = Set(config_json);
    }
    active.updated_at = Set(chrono::Utc::now());

    let model = active.update(&*db).await.map_err(|e| e.to_string())?;
    // 尝试转换为 McpServerConfig 并更新运行时
    let guard = mcp_state.read().await;
    if let Some(ref mcp_api) = *guard {
        match McpServerConfig::try_from(model.clone()) {
            Ok(config) => {
                mcp_api.update_server(&format!("{}", id), config).await?;
            }
            Err(e) => {
                tracing::warn!("更新 MCP 服务配置时转换失败: {}", e);
            }
        }
    }

    model_to_dto(model)
}
/// 删除一个配置
#[tauri::command]
pub async fn delete_mcp_serve_config(
    state: tauri::State<'_, DbState>,
    id: i32,
    mcp_state: tauri::State<'_, McpV2State>,
) -> Result<(), String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    msc::Entity::delete_by_id(id)
        .exec(&*db)
        .await
        .map_err(|e| e.to_string())?;

    let guard = mcp_state.read().await;
    if let Some(ref mcp_api) = *guard {
        mcp_api.remove_server(&format!("{}", id)).await?;
    }
    Ok(())
}
