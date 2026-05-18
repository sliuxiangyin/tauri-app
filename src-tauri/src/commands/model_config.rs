use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};

use crate::db::DbState;
use crate::entity::model_provider_config as mpc;
use crate::entity::model_provider_model as mpm;

// ---------------------------------------------------------------------------
// DTOs — 前端 ↔ Rust 的数据传输对象
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigDto {
    pub id: String,
    pub display_name: String,
    pub enabled: bool,
    pub provider_kind: String,
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub extra_json: Option<String>,
    pub sort_index: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelDto {
    pub id: String,
    pub config_id: String,
    pub model_id: String,
    pub model_name: String,
    pub group_name: String,
    pub sort_index: i32,
}

/// 列表返回：配置 + 其下模型数组
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigWithModels {
    #[serde(flatten)]
    pub config: ProviderConfigDto,
    pub models: Vec<ProviderModelDto>,
}

// ---------------------------------------------------------------------------
// 请求 Payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProviderConfigPayload {
    pub id: String,
    pub display_name: String,
    pub provider_kind: String,
    pub api_base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub extra_json: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProviderConfigPayload {
    pub display_name: Option<String>,
    pub enabled: Option<bool>,
    pub provider_kind: Option<String>,
    pub api_base_url: Option<String>,
    /// `None` = 不修改；`Some("")` = 清空为 NULL
    pub api_key: Option<String>,
    /// `None` = 不修改；`Some("")` = 清空为 NULL
    pub extra_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertProviderModelPayload {
    pub id: String,
    pub config_id: String,
    pub model_id: String,
    pub model_name: String,
    #[serde(default)]
    pub group_name: String,
    #[serde(default)]
    pub sort_index: i32,
}

// ---------------------------------------------------------------------------
// 转换助手
// ---------------------------------------------------------------------------

fn config_to_dto(m: mpc::Model) -> ProviderConfigDto {
    ProviderConfigDto {
        id: m.id,
        display_name: m.display_name,
        enabled: m.enabled != 0,
        provider_kind: m.provider_kind,
        api_base_url: m.api_base_url,
        api_key: m.api_key,
        extra_json: m.extra_json,
        sort_index: m.sort_index,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn model_to_dto(m: mpm::Model) -> ProviderModelDto {
    ProviderModelDto {
        id: m.id,
        config_id: m.config_id,
        model_id: m.model_id,
        model_name: m.model_name,
        group_name: m.group_name,
        sort_index: m.sort_index,
    }
}

fn opt_string_to_db(v: Option<String>) -> Option<String> {
    match v {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
    }
}

// ---------------------------------------------------------------------------
// Tauri 命令
// ---------------------------------------------------------------------------

/// 列出所有配置及其模型（可选仅启用的配置）。
#[tauri::command]
pub async fn list_provider_configs(
    state: tauri::State<'_, DbState>,
    enabled_only: Option<bool>,
) -> Result<Vec<ProviderConfigWithModels>, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let mut query = mpc::Entity::find().order_by_asc(mpc::Column::SortIndex);
    if enabled_only.unwrap_or(false) {
        query = query.filter(mpc::Column::Enabled.eq(1));
    }

    let configs = query
        .find_with_related(mpm::Entity)
        .all(&*db)
        .await
        .map_err(|e| e.to_string())?;

    let result = configs
        .into_iter()
        .map(|(config, mut models)| {
            models.sort_by_key(|m| m.sort_index);
            ProviderConfigWithModels {
                config: config_to_dto(config),
                models: models.into_iter().map(model_to_dto).collect(),
            }
        })
        .collect();

    Ok(result)
}

/// 创建新的提供商配置。
#[tauri::command]
pub async fn create_provider_config(
    state: tauri::State<'_, DbState>,
    payload: CreateProviderConfigPayload,
) -> Result<ProviderConfigDto, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let active = mpc::ActiveModel {
        id: Set(payload.id),
        display_name: Set(payload.display_name),
        enabled: Set(1),
        provider_kind: Set(payload.provider_kind),
        api_base_url: Set(payload.api_base_url),
        api_key: Set(payload.api_key),
        extra_json: Set(payload.extra_json),
        sort_index: Set(0),
        created_at: Set(chrono::Utc::now().timestamp()),
        updated_at: Set(chrono::Utc::now().timestamp()),
    };

    let model = active.insert(&*db).await.map_err(|e| e.to_string())?;
    Ok(config_to_dto(model))
}

/// 更新已有配置的部分字段。
#[tauri::command]
pub async fn update_provider_config(
    state: tauri::State<'_, DbState>,
    id: String,
    payload: UpdateProviderConfigPayload,
) -> Result<ProviderConfigDto, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let entity = mpc::Entity::find_by_id(&id)
        .one(&*db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("config not found: {id}"))?;

    let mut active: mpc::ActiveModel = entity.into();

    if let Some(v) = payload.display_name {
        active.display_name = Set(v);
    }
    if let Some(v) = payload.enabled {
        active.enabled = Set(if v { 1 } else { 0 });
    }
    if let Some(v) = payload.provider_kind {
        active.provider_kind = Set(v);
    }
    if let Some(v) = payload.api_base_url {
        active.api_base_url = Set(v);
    }
    if payload.api_key.is_some() {
        active.api_key = Set(opt_string_to_db(payload.api_key));
    }
    if payload.extra_json.is_some() {
        active.extra_json = Set(opt_string_to_db(payload.extra_json));
    }

    let model = active.update(&*db).await.map_err(|e| e.to_string())?;
    Ok(config_to_dto(model))
}

/// 删除一个配置（级联删除其下所有模型）。
#[tauri::command]
pub async fn delete_provider_config(
    state: tauri::State<'_, DbState>,
    id: String,
) -> Result<(), String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    mpc::Entity::delete_by_id(&id)
        .exec(&*db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 新增或更新一个模型（按 `id` 判断）。
#[tauri::command]
pub async fn upsert_provider_model(
    state: tauri::State<'_, DbState>,
    payload: UpsertProviderModelPayload,
) -> Result<ProviderModelDto, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let existing = mpm::Entity::find_by_id(&payload.id)
        .one(&*db)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(entity) = existing {
        let mut active: mpm::ActiveModel = entity.into();
        active.model_id = Set(payload.model_id);
        active.model_name = Set(payload.model_name);
        active.group_name = Set(payload.group_name);
        active.sort_index = Set(payload.sort_index);
        let model = active.update(&*db).await.map_err(|e| e.to_string())?;
        Ok(model_to_dto(model))
    } else {
        let active = mpm::ActiveModel {
            id: Set(payload.id),
            config_id: Set(payload.config_id),
            model_id: Set(payload.model_id),
            model_name: Set(payload.model_name),
            group_name: Set(payload.group_name),
            sort_index: Set(payload.sort_index),
        };
        let model = active.insert(&*db).await.map_err(|e| e.to_string())?;
        Ok(model_to_dto(model))
    }
}

/// 删除一个模型。
#[tauri::command]
pub async fn delete_provider_model(
    state: tauri::State<'_, DbState>,
    id: String,
) -> Result<(), String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    mpm::Entity::delete_by_id(&id)
        .exec(&*db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 批量更新配置的排序（`ids` 的顺序即为新 `sort_index`）。
#[tauri::command]
pub async fn reorder_provider_configs(
    state: tauri::State<'_, DbState>,
    ids: Vec<String>,
) -> Result<(), String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    for (i, id) in ids.iter().enumerate() {
        if let Some(entity) = mpc::Entity::find_by_id(id)
            .one(&*db)
            .await
            .map_err(|e| e.to_string())?
        {
            let mut active: mpc::ActiveModel = entity.into();
            active.sort_index = Set(i as i32);
            active.update(&*db).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 批量更新某配置下的模型排序。
#[tauri::command]
pub async fn reorder_provider_models(
    state: tauri::State<'_, DbState>,
    ids: Vec<String>,
) -> Result<(), String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    for (i, id) in ids.iter().enumerate() {
        if let Some(entity) = mpm::Entity::find_by_id(id)
            .one(&*db)
            .await
            .map_err(|e| e.to_string())?
        {
            let mut active: mpm::ActiveModel = entity.into();
            active.sort_index = Set(i as i32);
            active.update(&*db).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 桥接：从 DB 配置构建 ProviderConfigPayload（用于 LLM 调用）
// ---------------------------------------------------------------------------

/// 按配置 ID 解析为 `ProviderConfigPayload`，供 `llm_chat_once` / `llm_chat_stream` 使用。
///
/// 前端流程：`resolve_provider_payload(configId)` → 拿到 payload → 传入流式/非流式命令。
#[tauri::command]
pub async fn resolve_provider_payload(
    state: tauri::State<'_, DbState>,
    config_id: String,
) -> Result<crate::provider::llm::types::ProviderConfigPayload, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let entity = mpc::Entity::find_by_id(&config_id)
        .one(&*db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("config not found: {config_id}"))?;

    let payload = match entity.provider_kind.as_str() {
        "open_ai" | "openai_compatible" => {
            crate::provider::llm::types::ProviderConfigPayload::OpenAiCompatible {
                base_url: entity.api_base_url,
                api_key: entity.api_key.unwrap_or_default(),
            }
        }
        "anthropic" => crate::provider::llm::types::ProviderConfigPayload::Anthropic {
            api_key: entity.api_key.unwrap_or_default(),
        },
        "ollama" => crate::provider::llm::types::ProviderConfigPayload::Ollama {
            base_url: entity.api_base_url,
        },
        other => return Err(format!("unknown provider_kind: {other}")),
    };

    Ok(payload)
}
