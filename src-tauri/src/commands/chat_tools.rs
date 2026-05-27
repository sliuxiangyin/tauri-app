//! 聊天工具权限 Command 层
//!
//! 薄包装层 - 仅做参数转发和错误转换，业务逻辑在 services::chat_tools_service 中。

use std::sync::Arc;

use crate::provider::cache::Cache;
use crate::services::chat_tools_service::{self, ChatToolsConfig};
use tauri::State;

/// 获取指定 accountId + sessionId 的工具权限配置
#[tauri::command]
pub async fn get_chat_tools_config(
    cache: State<'_, Arc<Cache>>,
    account_id: String,
    session_id: String,
) -> Result<ChatToolsConfig, String> {
    Ok(chat_tools_service::get_tools_config(
        Arc::clone(&cache),
        account_id,
        session_id,
    ))
}

/// 保存指定 accountId + sessionId 的工具权限配置
#[tauri::command]
pub async fn save_chat_tools_config(
    cache: State<'_, Arc<Cache>>,
    account_id: String,
    session_id: String,
    config: ChatToolsConfig,
) -> Result<(), String> {
    chat_tools_service::set_tools_config(Arc::clone(&cache), account_id, session_id, config)
}

/// 删除指定 accountId + sessionId 的工具权限配置（恢复默认）
#[tauri::command]
pub async fn delete_chat_tools_config(
    cache: State<'_, Arc<Cache>>,
    account_id: String,
    session_id: String,
) -> Result<(), String> {
    chat_tools_service::remove_tools_config(Arc::clone(&cache), account_id, session_id)
}
