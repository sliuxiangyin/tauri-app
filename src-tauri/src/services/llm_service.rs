//! LLM 服务层
//!
//! 职责：
//! - 统一 LLM 流式调用入口
//! - 流式输出到前端（emit "llm:chunk"）
//! - 返回完整回复供调用方使用

use futures_util::StreamExt;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::{
    provider_trait::LlmProvider,
    types::{ChatMessage as LlmChatMessage, ChatRequest, ProviderConfigPayload, Role},
    LlmChunkEnvelope, LlmStreamEvent, Provider,
};
use crate::services::chat_model_service::get_account_model_config;

/// 流式聊天 - 实时推送前端，返回完整回复
///
/// # 参数
/// - `app`: Tauri AppHandle，用于 emit 事件到前端
/// - `cache`: Cache 句柄，用于获取模型配置
/// - `db_state`: 数据库状态
/// - `account_id`: 账户 ID
/// - `messages`: 聊天消息列表
/// - `system_prompt`: 系统提示词（可选）
///
/// # 返回
/// 完整回复字符串，流式结束后返回
pub async fn stream_chat(
    app: AppHandle,
    cache: Arc<Cache>,
    db_state: &DbState,
    account_id: &str,
    messages: Vec<ChatMessage>,
    system_prompt: Option<&str>,
) -> Result<String, String> {
    // 1. 构建消息列表
    let mut full_messages = Vec::new();
    
    if let Some(prompt) = system_prompt {
        full_messages.push(LlmChatMessage {
            role: Role::System,
            content: prompt.to_string(),
        });
    }
    
    for msg in messages {
        full_messages.push(msg);
    }

    // 2. 获取模型配置
    let (provider_config, model_id) = 
        get_account_model_config(cache, db_state, account_id).await?;

    // 3. 构建请求
    let req = ChatRequest {
        messages: full_messages,
        model: model_id,
        temperature: 0.8,
        max_tokens: None,
    };

    // 4. 创建 Provider 并获取流
    let provider = Provider::try_from(provider_config).map_err(|e| e.to_string())?;
    let mut stream = provider.stream_chat(req).await.map_err(|e| e.to_string())?;

    // 5. 流式循环：emit 给前端 + 收集完整回复
    let mut full_reply = String::new();

    while let Some(item) = stream.next().await {
        match item {
            Ok(LlmStreamEvent::TextDelta { text }) => {
                full_reply.push_str(&text);
                let _ = app.emit(
                    "llm:chunk",
                    &LlmChunkEnvelope::new(account_id.to_string(), LlmStreamEvent::TextDelta { text }),
                );
            }
            Ok(LlmStreamEvent::Done) => {
                let _ = app.emit(
                    "llm:chunk",
                    &LlmChunkEnvelope::new(account_id.to_string(), LlmStreamEvent::Done),
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "llm:error",
                    json!({
                        "account_id": account_id,
                        "message": e.to_string(),
                    }),
                );
                return Err(e.to_string());
            }
        }
    }

    Ok(full_reply)
}

/// 消息类型别名，简化 API
pub type ChatMessage = LlmChatMessage;

/// 获取账户的模型配置（Provider + model_id）
pub async fn get_provider_config(
    cache: Arc<Cache>,
    db_state: &DbState,
    account_id: &str,
) -> Result<(ProviderConfigPayload, String), String> {
    get_account_model_config(cache, db_state, account_id).await
}