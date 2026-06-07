//! Chat 模型选择服务
//!
//! 职责：
//! - 存储/获取 account_id 对应的模型选择
//! - 使用 sled 缓存持久化模型选择配置
//! - 提供获取当前账户模型的能力

use crate::provider::cache::{Cache, CacheError};
use crate::provider::llm::types::ProviderConfigPayload;
use crate::services::db::chat_model as chat_model_db;
use crate::services::traits::DbAccessor;
use crate::types::chat_model::{AccountModelSelection, ModelGroup, ModelItem};
use std::sync::Arc;

/// 缓存操作结果类型
type CacheResult<T> = std::result::Result<T, CacheError>;

const CHAT_MODEL_KEY_PREFIX: &str = "chat_model:";

/// Chat 模型服务
pub struct ChatModelService {
    cache: Arc<Cache>,
}

impl ChatModelService {
    /// 创建新的服务实例
    pub fn new(cache: Arc<Cache>) -> Self {
        Self { cache }
    }

    /// 保存账户的模型选择
    pub fn save_account_model(
        &self,
        account_id: &str,
        selection: &AccountModelSelection,
    ) -> CacheResult<()> {
        let key = format!("{}{}", CHAT_MODEL_KEY_PREFIX, account_id);
        let value = serde_json::to_vec(selection)?;
        self.cache.put(&key, value)
    }

    /// 获取账户的模型选择
    pub fn get_account_model(&self, account_id: &str) -> CacheResult<Option<AccountModelSelection>> {
        let key = format!("{}{}", CHAT_MODEL_KEY_PREFIX, account_id);
        match self.cache.get(&key)? {
            Some(v) => Ok(Some(serde_json::from_slice(&v)?)),
            None => Ok(None),
        }
    }
}

/// 获取当前账户的模型配置
///
/// 返回 (ProviderConfigPayload, model_id)
pub async fn get_account_model_config(
    cache: Arc<Cache>,
    db: &Arc<dyn DbAccessor>,
    account_id: &str,
) -> Result<(ProviderConfigPayload, String), String> {
    let service = ChatModelService::new(cache);

    // 尝试从缓存获取选择
    let selection = match service.get_account_model(account_id) {
        Ok(Some(s)) => s,
        _ => {
            // 未选择，返回第一个开启的模型
            let db_conn = db.get().await.map_err(|e| e.to_string())?;
            let model_info = chat_model_db::get_first_enabled_model(&*db_conn)
                .await
                .map_err(|e| e.to_string())?;
            let info = model_info.ok_or("no enabled model found")?;
            return Ok((info.payload, info.model_id));
        }
    };

    // 获取对应的 ProviderConfigPayload
    let db_conn = db.get().await.map_err(|e| e.to_string())?;
    let model_info = chat_model_db::get_model_by_ids(&*db_conn, &selection.config_id, &selection.model_id)
        .await
        .map_err(|e| e.to_string())?;

    match model_info {
        Some(info) => Ok((info.payload, selection.model_id)),
        None => Err("saved model not found".to_string()),
    }
}

/// 获取所有模型，按配置分组
pub async fn get_all_models_grouped(
    db: &Arc<dyn DbAccessor>,
) -> std::result::Result<Vec<ModelGroup>, String> {
    let db_conn = db.get().await.map_err(|e| e.to_string())?;

    use crate::entity::model_provider_config as mpc;
    use crate::entity::model_provider_model as mpm;
    use sea_orm::{EntityTrait, QueryOrder};

    // 查询所有配置及其模型
    let configs = mpc::Entity::find()
        .order_by_asc(mpc::Column::SortIndex)
        .find_with_related(mpm::Entity)
        .all(&*db_conn)
        .await
        .map_err(|e| e.to_string())?;

    let groups: Vec<ModelGroup> = configs
        .into_iter()
        .map(|(config, models)| {
            let mut sorted_models: Vec<_> = models.into_iter().collect();
            sorted_models.sort_by_key(|m| m.sort_index);

            let items: Vec<ModelItem> = sorted_models
                .into_iter()
                .map(|m| ModelItem {
                    model_id: m.model_id,
                    model_name: m.model_name,
                })
                .collect();

            ModelGroup {
                name: config.display_name,
                id: config.id,
                items,
            }
        })
        .collect();

    Ok(groups)
}

/// 获取账户的模型配置（兼容版）
///
/// 供 commands 层使用，返回 (ProviderConfigPayload, model_id)
pub async fn get_provider_config(
    cache: Arc<Cache>,
    db: &crate::db::DbState,
    account_id: &str,
) -> Result<(ProviderConfigPayload, String), String> {
    let db_accessor: Arc<dyn DbAccessor> = Arc::new(db.clone());
    get_account_model_config(cache, &db_accessor, account_id).await
}