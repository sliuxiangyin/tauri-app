//! 聊天消息相关命令
//! 提供消息的 CRUD 操作和 AI 对话接口

use nanoid::nanoid;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use tauri::State;

use crate::db::DbState;
use crate::entity::chat_message::{
    self, ActiveModel, CreateMessagePayload, Model as ChatMessageModel,
};
use crate::entity::ChatMessageEntity;

/// 生成唯一 ID
fn generate_id() -> String {
    nanoid!(21)
}

/// 消息 DTO（返回给前端）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub id: String,
    pub account_id: String,
    pub chat_type: String,
    pub session_id: String,
    pub parent_message_id: Option<String>,
    pub role: String,
    pub content: Option<String>,
    pub content_summary: Option<String>,
    pub thinking: Option<String>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_output: Option<String>,
    pub extends: String,
    pub attachments: Option<String>,
    pub status: String,
    pub token_usage: Option<String>,
    pub created_at: String,
    pub metadata: String,
    pub is_deleted: String,
}

impl From<ChatMessageModel> for MessageDto {
    fn from(model: ChatMessageModel) -> Self {
        Self {
            id: model.id,
            account_id: model.account_id,
            chat_type: model.chat_type,
            session_id: model.session_id,
            parent_message_id: model.parent_message_id,
            role: model.role,
            content: model.content,
            content_summary: model.content_summary,
            thinking: model.thinking,
            tool_calls: model.tool_calls,
            tool_call_id: model.tool_call_id,
            tool_output: model.tool_output,
            extends: model.extends,
            attachments: model.attachments,
            status: model.status,
            token_usage: model.token_usage,
            created_at: model.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            metadata: model.metadata,
            is_deleted: model.is_deleted,
        }
    }
}

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

    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    let mut query = ChatMessageEntity::find()
        .filter(chat_message::Column::AccountId.eq(&account_id))
        .filter(chat_message::Column::SessionId.eq(&session_id))
        .filter(chat_message::Column::IsDeleted.eq("0"))
        .order_by_desc(chat_message::Column::CreatedAt);

    // 可选过滤条件
    if let Some(ct) = chat_type {
        query = query.filter(chat_message::Column::ChatType.eq(ct));
    }

    let messages = query
        .limit(limit)
        .offset(offset)
        .all(&*db)
        .await
        .map_err(|e| e.to_string())?;

    // 反转顺序，使最新消息在最后
    let mut result: Vec<MessageDto> = messages.into_iter().map(MessageDto::from).collect();
    result.reverse();

    Ok(result)
}

/// 保存消息
#[tauri::command]
pub async fn save_message(
    state: State<'_, DbState>,
    payload: CreateMessagePayload,
) -> Result<MessageDto, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().naive_utc();

    let active_model = ActiveModel {
        id: Set(generate_id()),
        account_id: Set(payload.account_id),
        chat_type: Set(payload.chat_type),
        session_id: Set(payload.session_id),
        parent_message_id: Set(payload.parent_message_id),
        role: Set(payload.role),
        content: Set(Some(payload.content)),
        content_summary: Set(None),
        thinking: Set(payload.thinking),
        tool_calls: Set(payload.tool_calls),
        tool_call_id: Set(payload.tool_call_id),
        tool_output: Set(payload.tool_output),
        extends: Set(payload.extends.unwrap_or_else(|| "{}".to_string())),
        attachments: Set(None),
        status: Set(payload.status.unwrap_or_else(|| "completed".to_string())),
        token_usage: Set(None),
        created_at: Set(now),
        metadata: Set(payload.metadata.unwrap_or_else(|| "{}".to_string())),
        is_deleted: Set("0".to_string()),
    };

    let model = active_model.insert(&*db).await.map_err(|e| e.to_string())?;

    Ok(MessageDto::from(model))
}

/// 删除消息（软删除）
#[tauri::command]
pub async fn delete_message(state: State<'_, DbState>, message_id: String) -> Result<(), String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    let message = ChatMessageEntity::find_by_id(message_id.clone())
        .one(&*db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("message not found: {}", message_id))?;

    let mut active_model: ActiveModel = message.into();
    active_model.is_deleted = Set("1".to_string());

    active_model.update(&*db).await.map_err(|e| e.to_string())?;

    Ok(())
}

/// 获取会话列表（后期多会话支持）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDto {
    pub session_id: String,
    pub name: String,
    pub message_count: i64,
    pub last_message_at: Option<String>,
}

#[tauri::command]
pub async fn get_sessions(
    state: State<'_, DbState>,
    account_id: String,
) -> Result<Vec<SessionDto>, String> {
    let db = state.get().await.map_err(|e| e.to_string())?;

    // 目前只有 default 会话
    let messages = ChatMessageEntity::find()
        .filter(chat_message::Column::AccountId.eq(&account_id))
        .filter(chat_message::Column::IsDeleted.eq("0"))
        .order_by_desc(chat_message::Column::CreatedAt)
        .all(&*db)
        .await
        .map_err(|e| e.to_string())?;

    if messages.is_empty() {
        return Ok(vec![SessionDto {
            session_id: "default".to_string(),
            name: "默认会话".to_string(),
            message_count: 0,
            last_message_at: None,
        }]);
    }

    Ok(vec![SessionDto {
        session_id: "default".to_string(),
        name: "默认会话".to_string(),
        message_count: messages.len() as i64,
        last_message_at: messages
            .first()
            .map(|m: &ChatMessageModel| m.created_at.format("%Y-%m-%d %H:%M:%S").to_string()),
    }])
}

