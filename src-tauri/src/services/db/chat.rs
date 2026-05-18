//! 聊天消息数据库服务层
//! 提供消息的 CRUD 操作接口

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};

use crate::entity::chat_message::{self, ActiveModel, Model as ChatMessageModel};
use crate::entity::ChatMessageEntity;

/// 生成唯一 ID
fn generate_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 消息 DTO（返回给前端）
#[derive(Debug, Clone, serde::Serialize)]
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
            created_at: model.created_at.to_string(),
            metadata: model.metadata,
            is_deleted: model.is_deleted,
        }
    }
}

/// 会话 DTO
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionDto {
    pub session_id: String,
    pub name: String,
    pub message_count: i64,
    pub last_message_at: Option<String>,
}

/// 获取消息列表
pub async fn get_messages(
    db: &DatabaseConnection,
    account_id: String,
    session_id: Option<String>,
    chat_type: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<MessageDto>, String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    let mut query = ChatMessageEntity::find()
        .filter(chat_message::Column::AccountId.eq(&account_id))
        .filter(chat_message::Column::SessionId.eq(&session_id))
        .filter(chat_message::Column::IsDeleted.eq("0"))
        .order_by_asc(chat_message::Column::CreatedAt);

    if let Some(ct) = chat_type {
        query = query.filter(chat_message::Column::ChatType.eq(ct));
    }

    let messages = query
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;

    let result: Vec<MessageDto> = messages.into_iter().map(MessageDto::from).collect();

    Ok(result)
}

/// 保存消息
pub async fn save_message(
    db: &DatabaseConnection,
    payload: CreateMessagePayload,
) -> Result<MessageDto, String> {
    let now = chrono::Utc::now().timestamp();
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
    let model = active_model.insert(db).await.map_err(|e| e.to_string())?;
    Ok(MessageDto::from(model))
}

/// 删除消息（软删除）
#[allow(dead_code)]
pub async fn delete_message(db: &DatabaseConnection, message_id: String) -> Result<(), String> {
    let message = ChatMessageEntity::find_by_id(message_id.clone())
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("message not found: {}", message_id))?;

    let mut active_model: ActiveModel = message.into();
    active_model.is_deleted = Set("1".to_string());

    active_model.update(db).await.map_err(|e| e.to_string())?;

    Ok(())
}

/// 更新消息内容和状态
pub async fn update_message(
    db: &DatabaseConnection,
    message_id: String,
    content: Option<String>,
    status: Option<String>,
) -> Result<MessageDto, String> {
    let message = ChatMessageEntity::find_by_id(message_id.clone())
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("message not found: {}", message_id))?;

    let mut active_model: ActiveModel = message.into();

    if let Some(c) = content {
        active_model.content = Set(Some(c));
    }
    if let Some(s) = status {
        active_model.status = Set(s);
    }

    let model = active_model.update(db).await.map_err(|e| e.to_string())?;
    Ok(MessageDto::from(model))
}

/// 清空消息（硬删除所有指定账号/会话的消息）
pub async fn clear_messages(
    db: &DatabaseConnection,
    account_id: String,
    session_id: Option<String>,
    chat_type: Option<String>,
) -> Result<u64, String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());

    // 硬删除：使用 delete_many().filter().exec() 方式
    let mut delete_many = ChatMessageEntity::delete_many()
        .filter(chat_message::Column::AccountId.eq(&account_id))
        .filter(chat_message::Column::SessionId.eq(&session_id));

    if let Some(ref ct) = chat_type {
        delete_many = delete_many.filter(chat_message::Column::ChatType.eq(ct));
    }


    let delete_result = delete_many
        .exec(db)
        .await
        .map_err(|e| e.to_string())?;

    let deleted_count = delete_result.rows_affected;

    tracing::info!(
        "Hard deleted {} messages for account_id={}, session_id={}",
        deleted_count,
        account_id,
        session_id
    );

    Ok(deleted_count)
}

/// 获取会话列表
pub async fn get_sessions(db: &DatabaseConnection, account_id: String) -> Result<Vec<SessionDto>, String> {
    tracing::debug!("get_sessions: account_id = {}", account_id);
    let messages = ChatMessageEntity::find()
        .filter(chat_message::Column::AccountId.eq(&account_id))
        .filter(chat_message::Column::IsDeleted.eq("0"))
        .order_by_desc(chat_message::Column::CreatedAt)
        .all(db)
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
            .map(|m: &ChatMessageModel| m.created_at.to_string()),
    }])
}

// 重新导出 CreateMessagePayload 供 commands 层使用
pub use crate::entity::chat_message::CreateMessagePayload;