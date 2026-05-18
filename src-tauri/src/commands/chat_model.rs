//! Chat 模型选择命令
//!
//! 提供：
//! - 保存账户的模型选择
//! - 获取当前账户的模型（从缓存获取，未选择则返回第一个开启的模型）

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::types::ProviderConfigPayload;
use crate::services::chat_model_service::{AccountModelSelection, ChatModelService};
use crate::services::db::chat_model as chat_model_db;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

/// 账户模型信息 DTO（返回给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountModelDto {
    pub config_id: String,
    pub model_id: String,
    pub provider_payload: ProviderConfigPayload,
    pub is_default: bool,
}

/// 设置账户的模型选择
#[tauri::command]
pub async fn set_chat_model(
    cache: State<'_, Arc<Cache>>,
    db_state: State<'_, DbState>,
    account_id: String,
    config_id: String,
    model_id: String,
) -> Result<AccountModelDto, String> {
    let service = ChatModelService::new(cache.inner().clone());

    let selection = AccountModelSelection {
        config_id: config_id.clone(),
        model_id: model_id.clone(),
    };

    service
        .save_account_model(&account_id, &selection)
        .map_err(|e| e.to_string())?;

    // 获取 ProviderConfigPayload
    let model_info = chat_model_db::get_model_by_ids(&db_state, &config_id, &model_id)
        .await
        .map_err(|e| e.to_string())?;

    match model_info {
        Some(info) => Ok(AccountModelDto {
            config_id,
            model_id,
            provider_payload: info.payload,
            is_default: false,
        }),
        None => Err("model not found".to_string()),
    }
}

/// 获取当前账户的模型
///
/// 如果账户未选择过模型，则返回第一个开启的模型
#[tauri::command]
pub async fn get_chat_model(
    cache: State<'_, Arc<Cache>>,
    db_state: State<'_, DbState>,
    account_id: String,
) -> Result<AccountModelDto, String> {
    let service = ChatModelService::new(cache.inner().clone());

    // 先尝试从缓存获取
    match service.get_account_model(&account_id) {
        Ok(Some(selection)) => {
            // 获取对应的 ProviderConfigPayload
            let model_info = chat_model_db::get_model_by_ids(
                &db_state,
                &selection.config_id,
                &selection.model_id,
            )
            .await
            .map_err(|e| e.to_string())?;

            match model_info {
                Some(info) => Ok(AccountModelDto {
                    config_id: selection.config_id,
                    model_id: selection.model_id,
                    provider_payload: info.payload,
                    is_default: false,
                }),
                None => Err("saved model not found".to_string()),
            }
        }
        _ => {
            // 未选择过，返回第一个开启的模型
            let model_info =
                chat_model_db::get_first_enabled_model(&db_state)
                    .await
                    .map_err(|e| e.to_string())?;

            match model_info {
                Some(info) => Ok(AccountModelDto {
                    config_id: info.config_id,
                    model_id: info.model_id,
                    provider_payload: info.payload,
                    is_default: true,
                }),
                None => Err("no enabled model found".to_string()),
            }
        }
    }
}