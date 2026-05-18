use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::{
    provider_trait::LlmProvider,
    types::ChatRequest,
    types::ChatMessage,
};
use crate::services::llm_service;

#[tauri::command]
pub async fn llm_chat_once(
    cache: State<'_, Arc<Cache>>,
    db_state: State<'_, DbState>,
    account_id: String,
    mut req: ChatRequest,
) -> Result<String, String> {
    let (provider_config, model_id) = llm_service::get_provider_config(
        cache.inner().clone(),
        &db_state,
        &account_id,
    ).await?;
    req.model = model_id;

    let p = crate::provider::llm::Provider::try_from(provider_config).map_err(|e| e.to_string())?;
    p.send_message(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn llm_chat_stream(
    app: AppHandle,
    cache: State<'_, Arc<Cache>>,
    db_state: State<'_, DbState>,
    account_id: String,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    
    // 调用 llm_service 执行流式聊天，返回完整回复（丢弃）
    let _ = llm_service::stream_chat(
        app,
        cache.inner().clone(),
        &db_state,
        &account_id,
        messages,
        None, // commands 层不添加系统提示
    ).await?;
    
    Ok(())
}
