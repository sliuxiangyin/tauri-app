//! 消息数据库服务层
//! 提供 messages + conversations（内容块）的查询和 CRUD 接口

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect, Set,
};

use crate::entity::conversations::{self, Model as ConversationModel};
use crate::entity::message::{self, Model as MessageModel};
use crate::entity::plans::{self, Model as PlanModel};
use crate::entity::{ConversationEntity, MessageEntity, PlanEntity};

/// 生成唯一 ID
fn generate_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

// ──────────────────────────────────────────────────────────────
// DTO 定义
// ──────────────────────────────────────────────────────────────

/// 内容块 DTO（返回给前端）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentBlockDto {
    pub id: String,
    pub mid: String,
    pub block_type: String,
    pub order_num: i32,
    pub source: String,
    pub source_id: Option<String>,
    pub step_index: Option<i32>,
    pub content: Option<String>,
    pub content_summary: Option<String>,
    pub thinking: Option<String>,
    pub tool_name: Option<String>,
    pub tool_arguments: Option<String>,
    pub tool_output: Option<String>,
    pub tool_status: Option<String>,
    pub tool_duration_ms: Option<i64>,
    pub tool_error: Option<String>,
    pub extends: String,
    pub attachments: Option<String>,
    pub metadata: String,
    pub created_at: String,
}

impl From<ConversationModel> for ContentBlockDto {
    fn from(model: ConversationModel) -> Self {
        Self {
            id: model.id,
            mid: model.mid,
            block_type: model.block_type,
            order_num: model.order_num,
            source: model.source,
            source_id: model.source_id,
            step_index: model.step_index,
            content: model.content,
            content_summary: model.content_summary,
            thinking: model.thinking,
            tool_name: model.tool_name,
            tool_arguments: model.tool_arguments,
            tool_output: model.tool_output,
            tool_status: model.tool_status,
            tool_duration_ms: model.tool_duration_ms,
            tool_error: model.tool_error,
            extends: model.extends,
            attachments: model.attachments,
            metadata: model.metadata,
            created_at: model.created_at.to_string(),
        }
    }
}

/// Plan DTO（返回给前端）
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanDto {
    pub id: String,
    pub mid: String,
    pub need_agent: String,
    pub order_num: i32,
    pub reasoning: Option<String>,
    pub steps: Option<String>,
    pub step_results: Option<String>,
    pub stop_reason: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

impl From<PlanModel> for PlanDto {
    fn from(model: PlanModel) -> Self {
        Self {
            id: model.id,
            mid: model.mid,
            need_agent: model.need_agent,
            order_num: model.order_num,
            reasoning: model.reasoning,
            steps: model.steps,
            step_results: model.step_results,
            stop_reason: model.stop_reason,
            completed_at: model.completed_at.map(|t| t.to_string()),
            created_at: model.created_at.to_string(),
        }
    }
}

/// 统一内容项（用于按 order_num 排序 blocks 和 plan）
///
/// 序列化格式（邻接标签）：
/// `{ "type": "block", "data": { ...ContentBlockDto } }`
/// `{ "type": "plan",  "data": { ...PlanDto } }`
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ContentItem {
    #[serde(rename = "block")]
    Block(ContentBlockDto),
    #[serde(rename = "plan")]
    Plan(PlanDto),
}

impl ContentItem {
    /// 获取排序用的 order_num
    fn order_num(&self) -> i32 {
        match self {
            ContentItem::Block(b) => b.order_num,
            ContentItem::Plan(p) => p.order_num,
        }
    }
}

/// 消息 DTO（包含内容块嵌套）
#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageDto {
    pub id: String,
    pub account_id: String,
    pub chat_type: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub status: String,
    pub token_usage: Option<String>,
    pub created_at: String,
    pub is_deleted: String,
    /// 按 order_num 排序的统一内容序列（blocks + plan 合并）
    pub content: Vec<ContentItem>,
}

/// 会话 DTO
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionDto {
    pub session_id: String,
    pub name: String,
    pub message_count: i64,
    pub last_message_at: Option<String>,
}

// ──────────────────────────────────────────────────────────────
// 查询接口
// ──────────────────────────────────────────────────────────────

/// 获取消息列表（含内容块和 Plan）
///
/// 1. 查询 messages 索引表，按 created_at 排序
/// 2. 批量查询每条消息的 conversations 内容块（按 order_num 排序）
/// 3. 批量查询每条消息的 plan（如果有）
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

    // 1. 查询消息索引
    let mut query = MessageEntity::find()
        .filter(message::Column::AccountId.eq(&account_id))
        .filter(message::Column::SessionId.eq(&session_id))
        .filter(message::Column::IsDeleted.eq("0"))
        .order_by_asc(message::Column::CreatedAt);

    if let Some(ct) = chat_type {
        query = query.filter(message::Column::ChatType.eq(ct));
    }

    let  messages = query
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;

    if messages.is_empty() {
        return Ok(vec![]);
    }

    // 2. 收集所有 message ids
    let mids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();

    // 3. 批量查询所有关联的内容块
    let all_blocks = ConversationEntity::find()
        .filter(conversations::Column::Mid.is_in(&mids))
        .order_by_asc(conversations::Column::OrderNum)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;

    // 4. 批量查询所有关联的 Plan
    let all_plans = PlanEntity::find()
        .filter(plans::Column::Mid.is_in(&mids))
        .order_by_asc(plans::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;

    // 5. 组装结果
    let result: Vec<MessageDto> = messages
        .into_iter()
        .map(|msg| {
            // 收集该消息的内容块
            let blocks: Vec<ContentBlockDto> = all_blocks
                .iter()
                .filter(|b| b.mid == msg.id)
                .cloned()
                .map(ContentBlockDto::from)
                .collect();

            // 收集该消息的 plan（最多 1 个）
            let plan_dto: Option<PlanDto> = all_plans
                .iter()
                .find(|p| p.mid == msg.id)
                .cloned()
                .map(PlanDto::from);

            // 合并为统一有序序列
            let mut content: Vec<ContentItem> = Vec::with_capacity(blocks.len() + 1);
            for b in blocks {
                content.push(ContentItem::Block(b));
            }
            if let Some(p) = plan_dto {
                content.push(ContentItem::Plan(p));
            }
            // 按 order_num 稳定排序（同 order_num 时 plan 优先于 block）
            content.sort_by(|a, b| {
                a.order_num().cmp(&b.order_num()).then_with(|| match (a, b) {
                    (ContentItem::Plan(_), ContentItem::Block(_)) => std::cmp::Ordering::Less,
                    (ContentItem::Block(_), ContentItem::Plan(_)) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                })
            });

            MessageDto {
                id: msg.id,
                account_id: msg.account_id,
                chat_type: msg.chat_type,
                session_id: msg.session_id,
                parent_id: msg.parent_id,
                role: msg.role,
                status: msg.status,
                token_usage: msg.token_usage,
                created_at: msg.created_at.to_string(),
                is_deleted: msg.is_deleted,
                content,
            }
        })
        .collect();

    Ok(result)
}

/// 清空消息（硬删除 + 级联删除 conversations 和 plans）
pub async fn clear_messages(
    db: &DatabaseConnection,
    account_id: String,
    session_id: Option<String>,
    chat_type: Option<String>,
) -> Result<u64, String> {
    let session_id = session_id.unwrap_or_else(|| "default".to_string());

    // 1. 先查出所有要删除的 message ids
    let mut query = MessageEntity::find()
        .filter(message::Column::AccountId.eq(&account_id))
        .filter(message::Column::SessionId.eq(&session_id));

    if let Some(ref ct) = chat_type {
        query = query.filter(message::Column::ChatType.eq(ct));
    }

    let messages = query.all(db).await.map_err(|e| e.to_string())?;

    if messages.is_empty() {
        return Ok(0);
    }

    let mids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();

    // 2. 级联删除 conversations（内容块）
    ConversationEntity::delete_many()
        .filter(conversations::Column::Mid.is_in(&mids))
        .exec(db)
        .await
        .map_err(|e| e.to_string())?;

    // 3. 级联删除 plans
    PlanEntity::delete_many()
        .filter(plans::Column::Mid.is_in(&mids))
        .exec(db)
        .await
        .map_err(|e| e.to_string())?;

    // 4. 删除 messages
    let mut delete_query = MessageEntity::delete_many()
        .filter(message::Column::AccountId.eq(&account_id))
        .filter(message::Column::SessionId.eq(&session_id));

    if let Some(ct) = chat_type {
        delete_query = delete_query.filter(message::Column::ChatType.eq(ct));
    }

    let delete_result = delete_query.exec(db).await.map_err(|e| e.to_string())?;
    let deleted_count = delete_result.rows_affected;

    tracing::info!(
        "Hard deleted {} messages (with cascaded conversations & plans) for account_id={}, session_id={}",
        deleted_count,
        account_id,
        session_id
    );

    Ok(deleted_count)
}

/// 获取会话列表
pub async fn get_sessions(
    db: &DatabaseConnection,
    account_id: String,
) -> Result<Vec<SessionDto>, String> {
    tracing::debug!("get_sessions: account_id = {}", account_id);

    // 按 session_id 分组统计
    let messages = MessageEntity::find()
        .filter(message::Column::AccountId.eq(&account_id))
        .filter(message::Column::IsDeleted.eq("0"))
        .order_by_desc(message::Column::CreatedAt)
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

    // 按 session_id 分组
    let mut session_map: std::collections::HashMap<String, (i64, Option<String>)> =
        std::collections::HashMap::new();

    for msg in &messages {
        let entry = session_map
            .entry(msg.session_id.clone())
            .or_insert((0, None));
        entry.0 += 1;
        if entry.1.is_none() {
            entry.1 = Some(msg.created_at.to_string());
        }
    }

    let sessions: Vec<SessionDto> = session_map
        .into_iter()
        .map(|(session_id, (count, last_at))| SessionDto {
            session_id: session_id.clone(),
            name: if session_id == "default" {
                "默认会话".to_string()
            } else {
                format!("会话 {}", &session_id[..8.min(session_id.len())])
            },
            message_count: count,
            last_message_at: last_at,
        })
        .collect();

    Ok(sessions)
}

/// 保存消息索引
pub async fn save_message(
    db: &DatabaseConnection,
    payload: crate::entity::message::CreateMessagePayload,
) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let id = generate_id();
    let active_model = crate::entity::message::ActiveModel {
        id: Set(id.clone()),
        account_id: Set(payload.account_id),
        chat_type: Set(payload.chat_type),
        session_id: Set(payload.session_id),
        parent_id: Set(payload.parent_id),
        role: Set(payload.role),
        status: Set(payload.status.unwrap_or_else(|| "completed".to_string())),
        token_usage: Set(payload.token_usage),
        created_at: Set(now),
        is_deleted: Set("0".to_string()),
    };
    active_model.insert(db).await.map_err(|e| e.to_string())?;
    Ok(id)
}

/// 保存内容块
pub async fn save_conversation(
    db: &DatabaseConnection,
    payload: crate::entity::conversations::CreateConversationPayload,
) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let id = generate_id();
    let active_model = crate::entity::conversations::ActiveModel {
        id: Set(id.clone()),
        mid: Set(payload.mid),
        block_type: Set(payload.block_type),
        order_num: Set(payload.order_num),
        source: Set(payload.source),
        source_id: Set(payload.source_id),
        step_index: Set(payload.step_index),
        content: Set(payload.content),
        content_summary: Set(payload.content_summary),
        thinking: Set(payload.thinking),
        tool_name: Set(payload.tool_name),
        tool_arguments: Set(payload.tool_arguments),
        tool_output: Set(payload.tool_output),
        tool_status: Set(payload.tool_status),
        tool_duration_ms: Set(payload.tool_duration_ms),
        tool_error: Set(payload.tool_error),
        extends: Set(payload.extends.unwrap_or_else(|| "{}".to_string())),
        attachments: Set(payload.attachments),
        metadata: Set(payload.metadata.unwrap_or_else(|| "{}".to_string())),
        created_at: Set(now),
    };
    active_model.insert(db).await.map_err(|e| e.to_string())?;
    Ok(id)
}

/// 更新消息状态
pub async fn update_message_status(
    db: &DatabaseConnection,
    message_id: String,
    status: String,
) -> Result<(), String> {
    let message = MessageEntity::find_by_id(message_id.clone())
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("message not found: {}", message_id))?;

    let mut active_model = message.into_active_model();
    active_model.status = Set(status);

    active_model.update(db).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// 根据 mid 和 order_num 查找 conversation block
pub async fn get_conversation_by_order(
    db: &DatabaseConnection,
    mid: String,
    order_num: i32,
) -> Result<Option<ConversationModel>, String> {
    let block = ConversationEntity::find()
        .filter(conversations::Column::Mid.eq(mid))
        .filter(conversations::Column::OrderNum.eq(order_num))
        .one(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(block)
}

/// 更新 conversation block 的 content
pub async fn update_conversation_content(
    db: &DatabaseConnection,
    block_id: String,
    content: String,
) -> Result<(), String> {
    let block = ConversationEntity::find_by_id(block_id.clone())
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("conversation block not found: {}", block_id))?;

    let mut active_model = block.into_active_model();
    active_model.content = Set(Some(content));

    active_model.update(db).await.map_err(|e| e.to_string())?;
    Ok(())
}

// 重新导出 Payload 类型供上层使用
pub use crate::entity::conversations::CreateConversationPayload;
pub use crate::entity::message::CreateMessagePayload;
