//! 聊天消息相关命令
//! 提供消息的 CRUD 操作和 AI 对话接口
//! 具体实现委托至 services/db/chat.rs

use tauri::State;

use crate::db::DbState;
use crate::services::db::chat::{self, MessageDto, SessionDto};

/// 获取消息列表
#[tauri::command]
pub async fn get_messages(
    state: State<'_, DbState>,
    account_id: String,
    session_id: Option<String>,
    chat_type: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<MessageDto>, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    chat::get_messages(&*db, account_id, session_id, chat_type, limit, offset).await
}


/// 清空消息（软删除所有指定账号/会话的消息）
#[tauri::command]
pub async fn clear_messages(
    state: State<'_, DbState>,
    account_id: String,
    session_id: Option<String>,
    chat_type: Option<String>,
) -> Result<u64, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    chat::clear_messages(&*db, account_id, session_id, chat_type).await
}

/// 获取会话列表（后期多会话支持）
#[tauri::command]
pub async fn get_sessions(state: State<'_, DbState>, account_id: String) -> Result<Vec<SessionDto>, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;
    chat::get_sessions(&*db, account_id).await
}