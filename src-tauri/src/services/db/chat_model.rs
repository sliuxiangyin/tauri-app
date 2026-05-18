//! Chat 模型配置数据访问服务
//!
//! 提供模型配置的数据库查询接口，与业务逻辑解耦。

use crate::db::DbState;
use crate::entity::model_provider_config as mpc;
use crate::entity::model_provider_model as mpm;
use crate::provider::llm::types::ProviderConfigPayload;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use std::sync::Arc;

/// 第一个可用模型信息
pub struct FirstEnabledModel {
    pub config_id: String,
    pub model_id: String,
    pub model_name: String,
    pub display_name: String,
    pub payload: ProviderConfigPayload,
}

/// 获取账户的第一个可用模型（用于默认值）
///
/// 查询规则：
/// 1. 获取所有开启的模型提供配置（按 sort_index 排序）
/// 2. 返回第一个配置下按 sort_index 排序的第一个模型
pub async fn get_first_enabled_model(
    db_state: &DbState,
) -> Result<Option<FirstEnabledModel>, String> {
    let db: Arc<sea_orm::prelude::DatabaseConnection> = db_state
        .get()
        .await
        .map_err(|e| e.to_string())?;

    // 查询第一个开启的配置及其模型
    let configs = mpc::Entity::find()
        .filter(mpc::Column::Enabled.eq(1))
        .order_by_asc(mpc::Column::SortIndex)
        .find_with_related(mpm::Entity)
        .all(&*db)
        .await
        .map_err(|e| e.to_string())?;

    // 取第一个配置及其第一个模型
    if configs.is_empty() {
        return Ok(None);
    }

    let (config_model, models) = configs.into_iter().next().unwrap();
    let mut sorted_models: Vec<_> = models.into_iter().collect();
    sorted_models.sort_by_key(|m| m.sort_index);
    
    let first_model = sorted_models.into_iter().next();

    let (config_id, model_id, model_name) = match first_model {
        Some(m) => (config_model.id.clone(), m.model_id.clone(), m.model_name.clone()),
        None => return Ok(None),
    };

    let display_name = config_model.display_name.clone();

    // 构建 ProviderConfigPayload
    let payload = match config_model.provider_kind.as_str() {
        "open_ai" | "openai_compatible" => ProviderConfigPayload::OpenAiCompatible {
            base_url: config_model.api_base_url,
            api_key: config_model.api_key.unwrap_or_default(),
        },
        "anthropic" => ProviderConfigPayload::Anthropic {
            api_key: config_model.api_key.unwrap_or_default(),
        },
        "ollama" => ProviderConfigPayload::Ollama {
            base_url: config_model.api_base_url,
        },
        other => {
            return Err(format!("unknown provider_kind: {}", other));
        }
    };

    Ok(Some(FirstEnabledModel {
        config_id,
        model_id,
        model_name,
        display_name,
        payload,
    }))
}

/// 根据 config_id 和 model_id 获取模型信息
pub async fn get_model_by_ids(
    db_state: &DbState,
    config_id: &str,
    model_id: &str,
) -> Result<Option<FirstEnabledModel>, String> {
    let db: Arc<sea_orm::prelude::DatabaseConnection> = db_state
        .get()
        .await
        .map_err(|e| e.to_string())?;

    // 获取配置
    let config = mpc::Entity::find_by_id(config_id)
        .one(&*db)
        .await
        .map_err(|e| e.to_string())?;

    let config_model = match config {
        Some(c) => c,
        None => return Ok(None),
    };

    // 获取该配置下的所有模型
    let models = mpm::Entity::find()
        .filter(mpm::Column::ConfigId.eq(config_id))
        .all(&*db)
        .await
        .map_err(|e| e.to_string())?;

    // 找到匹配的模型
    let matching_model = models
        .into_iter()
        .find(|m| m.model_id == model_id);

    let (model_id_owned, model_name) = match matching_model {
        Some(m) => (m.model_id.clone(), m.model_name.clone()),
        None => return Ok(None),
    };

    let display_name = config_model.display_name.clone();

    // 构建 ProviderConfigPayload
    let payload = match config_model.provider_kind.as_str() {
        "open_ai" | "openai_compatible" => ProviderConfigPayload::OpenAiCompatible {
            base_url: config_model.api_base_url,
            api_key: config_model.api_key.unwrap_or_default(),
        },
        "anthropic" => ProviderConfigPayload::Anthropic {
            api_key: config_model.api_key.unwrap_or_default(),
        },
        "ollama" => ProviderConfigPayload::Ollama {
            base_url: config_model.api_base_url,
        },
        other => {
            return Err(format!("unknown provider_kind: {}", other));
        }
    };

    Ok(Some(FirstEnabledModel {
        config_id: config_id.to_string(),
        model_id: model_id_owned,
        model_name,
        display_name,
        payload,
    }))
}