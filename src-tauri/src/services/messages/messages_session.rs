//! 消息会话管理器
//!
//! 封装当前对话的内部状态，自动管理 message_id 和 block_order_num
//! 提供面向对象的 API，直接写入数据库，无需事件总线

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use sea_orm::DatabaseConnection;

use crate::entity::conversations::BlockType;
use crate::entity::plans::{CreatePlanPayload, UpdatePlanPayload};
use crate::provider::llm::types::IntentPlan;
use crate::services::db::message;
use crate::services::db::plans as plan_db;

use super::messages_event::{BlockAccumulator, BlockInfo, MessageStatus, ToolCallRecord};

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

    /// 当前 Plan ID（如果有）
    plan_id: Mutex<Option<String>>,
}

impl Clone for MessagesSession {
    fn clone(&self) -> Self {
        Self {
            message_id: self.message_id.clone(),
            account_id: self.account_id.clone(),
            session_id: self.session_id.clone(),
            chat_type: self.chat_type.clone(),
            block_order: AtomicI32::new(self.block_order.load(Ordering::SeqCst)),
            db: self.db.clone(),
            tool_accumulators: self.tool_accumulators.clone(),
            text_buffer: self.text_buffer.clone(),
            thinking_buffer: self.thinking_buffer.clone(),
            plan_id: Mutex::new(self.plan_id.lock().unwrap().clone()),
        }
    }
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
            plan_id: Mutex::new(None),
        })
    }

    /// 添加文本块（每次都新增记录）
    pub async fn add_text_block(&self, text: &str) -> BlockInfo {
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

        BlockInfo::new(BlockType::Text.as_str(), order_num)
    }

    /// 添加思考块
    pub async fn add_thinking_block(&self, text: &str) -> BlockInfo {
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

        BlockInfo::new(BlockType::Thinking.as_str(), order_num)
    }

    /// 添加工具调用记录（统一模型：包含调用信息 + 执行结果）
    ///
    /// 在 process_tool_batch 返回 tool_calls 后，由调用方统一入库。
    /// block_type 统一为 "tool"，包含完整的参数和结果。
    pub async fn add_tool(&self, record: &ToolCallRecord) -> BlockInfo {
        let order_num = self.block_order.fetch_add(1, Ordering::SeqCst);

        let tool_status = if record.success { "success" } else { "failed" };

        let payload = crate::entity::conversations::CreateConversationPayload {
            mid: self.message_id.clone(),
            block_type: BlockType::Tool.as_str().to_string(),
            order_num,
            content: None,
            thinking: None,
            tool_name: Some(record.name.clone()),
            tool_arguments: Some(record.arguments.to_string()),
            tool_output: Some(record.result.as_ref().map(|v| v.to_string()).unwrap_or_default()),
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
            tracing::error!("[MessagesSession] 保存工具块失败: {}", e);
        }

        tracing::debug!(
            "[MessagesSession] 保存工具: call_id={}, name={}, success={}",
            record.call_id,
            record.name,
            record.success
        );

        BlockInfo::new(BlockType::Tool.as_str(), order_num)
    }

    /// 标记工具执行错误
    #[allow(dead_code)]
    pub async fn mark_tool_error(&self, _call_id: &str, error: Option<String>) {
        let order_num = self.block_order.load(Ordering::SeqCst);

        let payload = crate::entity::conversations::CreateConversationPayload {
            mid: self.message_id.clone(),
            block_type: BlockType::Tool.as_str().to_string(),
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
    pub async fn flush_text_buffer(&self) -> Option<BlockInfo> {
        if self.text_buffer.is_empty() {
            return None;
        }

        let content = self.text_buffer.clone();
        let block_info = self.add_text_block(&content).await;

        Some(block_info)
    }

    /// 刷新思考缓冲（将累积的思考写入数据库）
    pub async fn flush_thinking_buffer(&self) -> Option<BlockInfo> {
        if self.thinking_buffer.is_empty() {
            return None;
        }

        let content = self.thinking_buffer.clone();
        let block_info = self.add_thinking_block(&content).await;

        Some(block_info)
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
    #[allow(dead_code)]
    pub fn current_message_id(&self) -> &str {
        &self.message_id
    }

    /// 获取下一块序号
    #[allow(dead_code)]
    pub fn next_order_num(&self) -> i32 {
        self.block_order.load(Ordering::SeqCst)
    }

    // ──────────────────────────────────────────────────────────────
    // Plan 管理
    // ──────────────────────────────────────────────────────────────

    /// 保存 Plan 到数据库
    ///
    /// 从 IntentPlan 创建并保存 Plan 记录，order_num 取自当前 block_order
    pub async fn save_plan(&self, intent_plan: &IntentPlan) -> Result<String, String> {
        // plan 的 order_num 取自当前 block_order，然后递增 block_order
        // 保证后续 blocks 的 order_num 大于 plan
        let order_num = self.block_order.fetch_add(1, Ordering::SeqCst);

        let payload = CreatePlanPayload::from_intent_plan(self.message_id.clone(), intent_plan)
            .with_order_num(order_num);
        let plan_id = plan_db::save_plan(&*self.db, payload).await?;

        // 保存 plan_id 到结构体（使用代码块限制 guard 作用域）
        {
            let mut guard = self.plan_id.lock().unwrap();
            *guard = Some(plan_id.clone());
        }

        tracing::debug!("[MessagesSession] 保存 Plan 成功: plan_id={}", plan_id);
        Ok(plan_id)
    }

    /// 更新 Plan 结果
    ///
    /// 在 Plan 执行完成后调用，更新 step_results 和 stop_reason
    pub async fn update_plan_result(
        &self,
        step_results_json: Option<String>,
        stop_reason: &str,
    ) -> Result<(), String> {
        // 从 plan_id 获取 plan_id（使用代码块限制 guard 作用域）
        let plan_id = {
            let guard = self.plan_id.lock().unwrap();
            guard.as_ref()
                .ok_or_else(|| "No plan to update".to_string())?
                .clone()
        };  // guard 在这里自动 drop

        let payload = UpdatePlanPayload::new()
            .with_step_results_json(step_results_json)
            .with_stop_reason_str(stop_reason.to_string());

        plan_db::update_plan(&*self.db, plan_id, payload).await?;

        tracing::debug!("[MessagesSession] 更新 Plan 结果成功");
        Ok(())
    }

    /// 获取当前 Plan ID
    #[allow(dead_code)]
    pub fn current_plan_id(&self) -> Option<String> {
        self.plan_id.lock().unwrap().clone()
    }
}

// ──────────────────────────────────────────────────────────────
// UpdatePlanPayload 扩展方法
// ──────────────────────────────────────────────────────────────

impl UpdatePlanPayload {
    /// 设置步骤结果（从 JSON 字符串）
    pub fn with_step_results_json(mut self, results: Option<String>) -> Self {
        self.step_results = results;
        self
    }

    /// 设置停止原因（从字符串）
    pub fn with_stop_reason_str(mut self, reason: String) -> Self {
        self.stop_reason = Some(reason);
        self
    }
}
