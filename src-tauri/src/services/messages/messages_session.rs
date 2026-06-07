//! 消息会话管理器
//!
//! 封装当前对话的内部状态，自动管理 message_id 和 block_order_num
//! 提供面向对象的 API，直接写入数据库，无需事件总线

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use sea_orm::DatabaseConnection;

use crate::entity::conversations::BlockType;
use crate::services::db::message;

use super::messages_event::{BlockAccumulator, MessageStatus};

/// 消息会话
///
/// 保存当前对话的内部状态：
/// - 当前消息 ID
/// - 块序号（自动递增）
/// - 工具调用累加器（用于聚合增量）
/// - 文本/思考缓冲
pub struct MessagesSession {
    /// 当前消息 ID（由内部保存，无需外部传递）
    pub message_id: String,

    /// 账号 ID
    pub account_id: String,
    /// 会话 ID
    pub session_id: String,
    /// 聊天类型
    pub chat_type: String,

    /// 块序号（自动递增）
    block_order: AtomicI32,

    /// 数据库连接
    db: Arc<DatabaseConnection>,

    /// 工具调用块累加器（call_id -> accumulator）
    tool_accumulators: HashMap<String, BlockAccumulator>,

    /// 文本缓冲（用于聚合 TextDelta）
    text_buffer: String,
    /// 思考缓冲（用于聚合 ReasoningDelta）
    thinking_buffer: String,

    /// 第一个 Text block 的 ID（用于后续更新）
    first_text_block_id: Mutex<Option<String>>,
}

impl MessagesSession {
    /// 创建新会话（异步）
    ///
    /// 内部同时创建：
    /// - user 消息 → messages 表 + conversations 表（Text block）
    /// - assistant 占位消息 → messages 表（status=pending）
    ///
    /// 返回的 message_id 指向 assistant 占位消息
    pub async fn new(
        account_id: String,
        chat_type: String,
        session_id: String,
        content: String,
        db: Arc<DatabaseConnection>,
    ) -> Result<Self, String> {
        // 1. 创建 user 消息索引
        let user_payload = crate::entity::message::CreateMessagePayload {
            account_id: account_id.clone(),
            chat_type: chat_type.clone(),
            session_id: session_id.clone(),
            role: "user".to_string(),
            parent_id: None,
            status: Some("completed".to_string()),
            token_usage: None,
        };
        let _user_message_id = message::save_message(&*db, user_payload).await?;

        // 2. 创建 user 内容块（Text 类型）
        let user_conversation_payload = crate::entity::conversations::CreateConversationPayload {
            mid: _user_message_id.clone(),
            block_type: BlockType::Text.as_str().to_string(),
            order_num: 0,
            source: "chat".to_string(),
            source_id: None,
            step_index: None,
            content: Some(content),
            content_summary: None,
            thinking: None,
            tool_name: None,
            tool_arguments: None,
            tool_output: None,
            tool_status: None,
            tool_duration_ms: None,
            tool_error: None,
            extends: None,
            attachments: None,
            metadata: None,
        };
        if let Err(e) = message::save_conversation(&*db, user_conversation_payload).await {
            tracing::error!("[MessagesSession] 保存用户消息内容块失败: {}", e);
        }

        // 3. 创建 assistant 占位消息
        let user_message_id_for_parent = _user_message_id.clone();
        let assistant_payload = crate::entity::message::CreateMessagePayload {
            account_id: account_id.clone(),
            chat_type: chat_type.clone(),
            session_id: session_id.clone(),
            role: "assistant".to_string(),
            parent_id: Some(user_message_id_for_parent),
            status: Some("pending".to_string()),
            token_usage: None,
        };
        let message_id = message::save_message(&*db, assistant_payload).await?;

        // 4. 创建 assistant 空 Text block（用于后续更新）
        let assistant_conversation_payload = crate::entity::conversations::CreateConversationPayload {
            mid: message_id.clone(),
            block_type: BlockType::Text.as_str().to_string(),
            order_num: 0,
            source: "chat".to_string(),
            source_id: None,
            step_index: None,
            content: Some(String::new()), // 空内容，后续 add_text_block 会更新
            content_summary: None,
            thinking: None,
            tool_name: None,
            tool_arguments: None,
            tool_output: None,
            tool_status: None,
            tool_duration_ms: None,
            tool_error: None,
            extends: None,
            attachments: None,
            metadata: None,
        };
        let first_text_block_id = message::save_conversation(&*db, assistant_conversation_payload)
            .await
            .ok();

        tracing::debug!("[MessagesSession] 创建会话成功: user_id={}, assistant_id={}", _user_message_id, message_id);

        Ok(Self {
            message_id,
            account_id,
            session_id,
            chat_type,
            block_order: AtomicI32::new(0),
            db,
            tool_accumulators: HashMap::new(),
            text_buffer: String::new(),
            thinking_buffer: String::new(),
            first_text_block_id: Mutex::new(first_text_block_id),
        })
    }

    /// 添加文本块
    pub async fn add_text_block(&self, text: &str) -> i32 {
        // 如果是第一个 Text block（order_num=0），则更新已有 block
        let block_id_to_update = {
            let mut block_id_guard = self.first_text_block_id.lock().unwrap();
            block_id_guard.take()
        };

        if let Some(block_id) = block_id_to_update {
            if let Err(e) = message::update_conversation_content(&*self.db, block_id, text.to_string()).await {
                tracing::error!("[MessagesSession] 更新文本块失败: {}", e);
            }
            return 0;
        }

        let order_num = self.block_order.fetch_add(1, Ordering::SeqCst);

        let payload = crate::entity::conversations::CreateConversationPayload {
            mid: self.message_id.clone(),
            block_type: BlockType::Text.as_str().to_string(),
            order_num,
            content: Some(text.to_string()),
            thinking: None,
            tool_name: None,
            tool_arguments: None,
            tool_output: None,
            tool_status: None,
            source: "chat".to_string(),
            source_id: None,
            step_index: None,
            content_summary: None,
            tool_duration_ms: None,
            tool_error: None,
            extends: None,
            attachments: None,
            metadata: None,
        };

        if let Err(e) = message::save_conversation(&*self.db, payload).await {
            tracing::error!("[MessagesSession] 保存文本块失败: {}", e);
        }

        order_num
    }

    /// 添加思考块
    pub async fn add_thinking_block(&self, text: &str) -> i32 {
        let order_num = self.block_order.fetch_add(1, Ordering::SeqCst);

        let payload = crate::entity::conversations::CreateConversationPayload {
            mid: self.message_id.clone(),
            block_type: BlockType::Thinking.as_str().to_string(),
            order_num,
            content: None,
            thinking: Some(text.to_string()),
            tool_name: None,
            tool_arguments: None,
            tool_output: None,
            tool_status: None,
            source: "chat".to_string(),
            source_id: None,
            step_index: None,
            content_summary: None,
            tool_duration_ms: None,
            tool_error: None,
            extends: None,
            attachments: None,
            metadata: None,
        };

        if let Err(e) = message::save_conversation(&*self.db, payload).await {
            tracing::error!("[MessagesSession] 保存思考块失败: {}", e);
        }

        order_num
    }

    /// 处理工具调用开始
    pub async fn on_tool_call_start(&mut self, call_id: String, name: String) -> i32 {
        let order_num = self.block_order.fetch_add(1, Ordering::SeqCst);

        // 创建累加器
        let mut accumulator = BlockAccumulator::default();
        accumulator.tool_name = Some(name.clone());
        self.tool_accumulators.insert(call_id, accumulator);

        let payload = crate::entity::conversations::CreateConversationPayload {
            mid: self.message_id.clone(),
            block_type: BlockType::ToolCall.as_str().to_string(),
            order_num,
            content: None,
            thinking: None,
            tool_name: Some(name),
            tool_arguments: None,
            tool_output: None,
            tool_status: Some("pending".to_string()),
            source: "chat".to_string(),
            source_id: None,
            step_index: None,
            content_summary: None,
            tool_duration_ms: None,
            tool_error: None,
            extends: None,
            attachments: None,
            metadata: None,
        };

        if let Err(e) = message::save_conversation(&*self.db, payload).await {
            tracing::error!("[MessagesSession] 保存工具调用块失败: {}", e);
        }

        order_num
    }

    /// 处理工具参数增量
    pub fn on_tool_call_delta(&mut self, call_id: &str, arguments: &str) {
        if let Some(acc) = self.tool_accumulators.get_mut(call_id) {
            acc.add_arguments(arguments);
        }
    }

    /// 处理工具调用完成（参数收集完毕）
    pub fn on_tool_call_done(&mut self, call_id: &str) -> Option<i32> {
        let accumulator = self.tool_accumulators.remove(call_id)?;
        let order_num = self.block_order.load(Ordering::SeqCst);
        tracing::debug!(
            "[MessagesSession] 工具调用完成: call_id={}, args_len={}",
            call_id,
            accumulator.tool_arguments.len()
        );
        Some(order_num)
    }

    /// 处理工具执行结果
    pub async fn on_tool_result(
        &mut self,
        _call_id: String,
        name: String,
        output: serde_json::Value,
        success: bool,
    ) -> i32 {
        let order_num = self.block_order.fetch_add(1, Ordering::SeqCst);

        let tool_status = if success { "success" } else { "failed" };

        let payload = crate::entity::conversations::CreateConversationPayload {
            mid: self.message_id.clone(),
            block_type: BlockType::ToolResult.as_str().to_string(),
            order_num,
            content: None,
            thinking: None,
            tool_name: Some(name),
            tool_arguments: None,
            tool_output: Some(output.to_string()),
            tool_status: Some(tool_status.to_string()),
            source: "chat".to_string(),
            source_id: None,
            step_index: None,
            content_summary: None,
            tool_duration_ms: None,
            tool_error: None,
            extends: None,
            attachments: None,
            metadata: None,
        };

        if let Err(e) = message::save_conversation(&*self.db, payload).await {
            tracing::error!("[MessagesSession] 保存工具结果块失败: {}", e);
        }

        order_num
    }

    /// 标记工具执行错误
    pub async fn mark_tool_error(&self, _call_id: &str, error: Option<String>) {
        let order_num = self.block_order.load(Ordering::SeqCst);

        let payload = crate::entity::conversations::CreateConversationPayload {
            mid: self.message_id.clone(),
            block_type: BlockType::ToolResult.as_str().to_string(),
            order_num,
            content: None,
            thinking: None,
            tool_name: None,
            tool_arguments: None,
            tool_output: None,
            tool_status: Some("failed".to_string()),
            source: "chat".to_string(),
            source_id: None,
            step_index: None,
            content_summary: None,
            tool_duration_ms: None,
            tool_error: error,
            extends: None,
            attachments: None,
            metadata: None,
        };

        if let Err(e) = message::save_conversation(&*self.db, payload).await {
            tracing::error!("[MessagesSession] 保存工具错误块失败: {}", e);
        }
    }

    /// 刷新文本缓冲（将累积的文本写入数据库）
    pub async fn flush_text_buffer(&self) -> Option<i32> {
        if self.text_buffer.is_empty() {
            return None;
        }

        let content = self.text_buffer.clone();
        let order_num = self.add_text_block(&content).await;

        Some(order_num)
    }

    /// 刷新思考缓冲（将累积的思考写入数据库）
    pub async fn flush_thinking_buffer(&self) -> Option<i32> {
        if self.thinking_buffer.is_empty() {
            return None;
        }

        let content = self.thinking_buffer.clone();
        let order_num = self.add_thinking_block(&content).await;

        Some(order_num)
    }

    /// 完成会话（刷新缓冲 + 更新消息状态）
    pub async fn complete(&self, status: MessageStatus) {
        // 先刷新缓冲
        let _ = self.flush_text_buffer().await;
        let _ = self.flush_thinking_buffer().await;

        // 更新消息状态
        if let Err(e) =
            message::update_message_status(&*self.db, self.message_id.clone(), status.as_str().to_string()).await
        {
            tracing::error!("[MessagesSession] 更新消息状态失败: {}", e);
        }
    }

    /// 获取当前消息 ID
    pub fn current_message_id(&self) -> &str {
        &self.message_id
    }

    /// 获取下一块序号
    pub fn next_order_num(&self) -> i32 {
        self.block_order.load(Ordering::SeqCst)
    }
}
