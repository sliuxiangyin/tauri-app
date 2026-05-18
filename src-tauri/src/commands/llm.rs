use futures_util::StreamExt;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::{
    provider_trait::LlmProvider, types::{ChatRequest, ProviderConfigPayload},
    LlmChunkEnvelope, LlmStreamEvent, Provider,
};
use crate::services::chat_model_service::ChatModelService;
use crate::services::db::chat_model as chat_model_db;

/// 获取当前账户的模型配置
async fn get_account_model_config(
    cache: Arc<Cache>,
    db_state: &DbState,
    account_id: &str,
) -> Result<(ProviderConfigPayload, String), String> {
    let service = ChatModelService::new(cache);

    // 尝试从缓存获取选择
    let selection = match service.get_account_model(account_id) {
        Ok(Some(s)) => s,
        _ => {
            // 未选择，返回第一个开启的模型
            let model_info = chat_model_db::get_first_enabled_model(db_state)
                .await
                .map_err(|e| e.to_string())?;
            let info = model_info.ok_or("no enabled model found")?;
            return Ok((info.payload, info.model_id));
        }
    };

    // 获取对应的 ProviderConfigPayload
    let model_info = chat_model_db::get_model_by_ids(db_state, &selection.config_id, &selection.model_id)
        .await
        .map_err(|e| e.to_string())?;

    match model_info {
        Some(info) => Ok((info.payload, selection.model_id)),
        None => Err("saved model not found".to_string()),
    }
}

#[tauri::command]
pub async fn llm_chat_once(
    cache: State<'_, Arc<Cache>>,
    db_state: State<'_, DbState>,
    account_id: String,
    mut req: ChatRequest,
) -> Result<String, String> {
    let (provider_config, model_id) = get_account_model_config(cache.inner().clone(), &db_state, &account_id).await?;
    req.model = model_id;

    let p = Provider::try_from(provider_config).map_err(|e| e.to_string())?;
    p.send_message(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn llm_chat_stream(
    app: AppHandle,
    stream_id: String,
    cache: State<'_, Arc<Cache>>,
    db_state: State<'_, DbState>,
    account_id: String,
    mut req: ChatRequest,
) -> Result<(), String> {
    let stream_id = stream_id.trim();
    if stream_id.is_empty() {
        return Err("stream_id must be non-empty".to_string());
    }
    let stream_id = stream_id.to_string();

    let (provider_config, model_id) = get_account_model_config(cache.inner().clone(), &db_state, &account_id).await?;
    req.model = model_id;

    let p = Provider::try_from(provider_config).map_err(|e| e.to_string())?;
    let mut stream = p.stream_chat(req).await.map_err(|e| e.to_string())?;
    while let Some(item) = stream.next().await {
        match item {
            Ok(ev) => {
                let _ = app.emit("llm:chunk", &LlmChunkEnvelope::new(stream_id.clone(), ev));
            }
            Err(e) => {
                let _ = app.emit(
                    "llm:error",
                    json!({
                        "stream_id": &stream_id,
                        "message": e.to_string(),
                    }),
                );
                let _ = app.emit(
                    "llm:chunk",
                    &LlmChunkEnvelope::new(stream_id.clone(), LlmStreamEvent::Done),
                );
                return Ok(());
            }
        }
    }
    Ok(())
}
