use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::{AppHandle, Emitter, State};

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::{
    ChatMessage, ChatRequest, LlmChunkEnvelope, LlmProvider, LlmStreamSender,
};
use crate::provider::mcp::McpManager;
use crate::services::llm_service;
use crate::types::chat::ChatContext;

/// LLM 流的取消标记管理
pub struct LlmAbortFlags {
    flags: RwLock<HashMap<String, Arc<AtomicBool>>>,
}

impl LlmAbortFlags {
    pub fn new() -> Self {
        Self {
            flags: RwLock::new(HashMap::new()),
        }
    }

    /// 为账户创建新的 abort_flag
    pub async fn create(&self, account_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.flags.write().await.insert(account_id.to_string(), flag.clone());
        flag
    }

    /// 获取账户的 abort_flag
    pub async fn get(&self, account_id: &str) -> Option<Arc<AtomicBool>> {
        self.flags.read().await.get(account_id).cloned()
    }

    /// 取消账户的流
    pub async fn cancel(&self, account_id: &str) -> bool {
        if let Some(flag) = self.flags.write().await.remove(account_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}

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
    _cache: State<'_, Arc<Cache>>,
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    abort_flags: State<'_, LlmAbortFlags>,
    account_id: String,
    session_id: String,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    // 创建 abort flag
    let abort_flag = abort_flags.create(&account_id).await;
    let account_id_for_ctx = account_id.clone();

    // 创建流式事件通道，由命令层统一消费并 emit 到前端
    let (tx, mut rx): (LlmStreamSender, _) = tokio::sync::mpsc::unbounded_channel();

    let app_clone = app.clone();
    let account_id_clone = account_id.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            let _ = app_clone.emit(
                "llm:chunk",
                &LlmChunkEnvelope::new(account_id_clone.clone(), event),
            );
        }
    });

    let ctx = ChatContext {
        account_id: account_id_for_ctx,
        chat_type: "client".to_string(),
        session_id,
        messages,
    };

    let result = llm_service::chat_with_placeholder(
        &db_state,
        (*mcp_manager).clone(),
        ctx,
        Some(tx),
        abort_flag,
    )
    .await;

    // 流结束后清理 abort_flag（无论成功还是失败）
    let account_id_for_cleanup = account_id.clone();
    abort_flags.flags.write().await.remove(&account_id_for_cleanup);

    result.map(|_| ())
}

/// 取消指定账户的 LLM 流式响应
#[tauri::command]
pub async fn llm_chat_cancel(
    abort_flags: State<'_, LlmAbortFlags>,
    account_id: String,
) -> Result<bool, String> {
    let cancelled = abort_flags.cancel(&account_id).await;
    println!("[Command] llm_chat_cancel for {}: {}", account_id, cancelled);
    Ok(cancelled)
}

