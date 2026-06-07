//! 消息服务层（MessagesService）
//!
//! 职责：
//! - 封装消息查询、清空、会话列表等业务逻辑
//! - 通过构造器注入 DbAccessor，实现解耦和可测试
//! - 协调 messages + conversations + plans 的查询

use std::sync::Arc;

use crate::services::db::message::{self, MessageDto, SessionDto};
use crate::services::traits::DbAccessor;

/// 消息服务
///
/// 通过构造器注入依赖，支持可测试化
pub struct MessagesService {
    db: Arc<dyn DbAccessor>,
}

impl MessagesService {
    /// 创建新的消息服务
    pub fn new(db: Arc<dyn DbAccessor>) -> Self {
        Self { db }
    }

    /// 获取消息列表（含内容块和 Plan）
    pub async fn get_messages(
        &self,
        account_id: String,
        session_id: Option<String>,
        chat_type: Option<String>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<MessageDto>, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        message::get_messages(&*db, account_id, session_id, chat_type, limit, offset).await
    }

    /// 清空消息（硬删除 + 级联删除 conversations 和 plans）
    pub async fn clear_messages(
        &self,
        account_id: String,
        session_id: Option<String>,
        chat_type: Option<String>,
    ) -> Result<u64, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        message::clear_messages(&*db, account_id, session_id, chat_type).await
    }

    /// 获取会话列表
    pub async fn get_sessions(&self, account_id: String) -> Result<Vec<SessionDto>, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        message::get_sessions(&*db, account_id).await
    }
}
