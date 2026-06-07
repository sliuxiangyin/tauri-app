//! 消息相关命令（v2 - 基于 messages + conversations 新表结构）
//! 提供消息列表、清空消息、会话列表接口
//! 具体实现委托至 services/messages_service.rs

use std::sync::Arc;
use tauri::State;

use crate::db::DbState;
use crate::services::db::message::{MessageDto, SessionDto};
use crate::services::messages_service::MessagesService;

/// 从 DbState 构建 MessagesService
fn build_service(state: &State<'_, DbState>) -> Result<MessagesService, String> {
    let db_state = state.inner().clone();
    Ok(MessagesService::new(Arc::new(db_state)))
}

/// 获取消息列表（含内容块和 Plan）
#[tauri::command]
pub async fn get_messages(
    state: State<'_, DbState>,
    account_id: String,
    session_id: Option<String>,
    chat_type: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<MessageDto>, String> {
    let service = build_service(&state)?;
    service.get_messages(account_id, session_id, chat_type, limit, offset).await
}

/// 清空消息（硬删除 + 级联删除 conversations 和 plans）
#[tauri::command]
pub async fn clear_messages(
    state: State<'_, DbState>,
    account_id: String,
    session_id: Option<String>,
    chat_type: Option<String>,
) -> Result<u64, String> {
    let service = build_service(&state)?;
    service.clear_messages(account_id, session_id, chat_type).await
}

/// 获取会话列表
#[tauri::command]
pub async fn get_sessions(
    state: State<'_, DbState>,
    account_id: String,
) -> Result<Vec<SessionDto>, String> {
    let service = build_service(&state)?;
    service.get_sessions(account_id).await
}
