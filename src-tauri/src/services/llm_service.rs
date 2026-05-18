//! LLM 服务层
//!
//! 职责：
//! - 统一 LLM 流式调用入口
//! - 流式输出通过外部传入的 `stream_sender` 转发，调用方统一处理 emit
//! - 返回完整回复供调用方使用
//! - 统一消息占位+状态机管理

use futures_util::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::{
    provider_trait::LlmProvider,
    types::{ChatMessage as LlmChatMessage, ChatRequest, ProviderConfigPayload, Role},
    LlmStreamEvent, LlmStreamSender, Provider,
};
use crate::services::chat_model_service::get_account_model_config;
use crate::services::db::chat::{self, CreateMessagePayload};
use crate::types::chat::ChatContext;

/// 流式聊天 - 通过外部通道转发流式事件，返回完整回复
///
/// # 参数
/// - `db_state`: 数据库状态
/// - `account_id`: 账户 ID
/// - `messages`: 聊天消息列表
/// - `system_prompt`: 系统提示词（可选）
/// - `stream_sender`: 流式事件发送通道（可选），存在时转发 `LlmStreamEvent`
/// - `abort_flag`: 取消标记，用于中断流式响应
///
/// # 返回
/// 完整回复字符串，流式结束后返回
pub async fn stream_chat(
    db_state: &DbState,
    account_id: &str,
    messages: Vec<ChatMessage>,
    system_prompt: Option<&str>,
    stream_sender: Option<LlmStreamSender>,
    abort_flag: Arc<AtomicBool>,
) -> Result<String, String> {
    // 获取全局 Cache 单例
    let cache = Cache::get_global().map_err(|e| e.to_string())?;
    // 1. 构建消息列表
    let mut full_messages = Vec::new();

    if let Some(prompt) = system_prompt {
        full_messages.push(LlmChatMessage {
            role: Role::System,
            content: prompt.to_string(),
        });
    }

    for msg in messages {
        full_messages.push(msg);
    }

    // 2. 获取模型配置
    let (provider_config, model_id) =
        get_account_model_config(cache, db_state, account_id).await?;
    // 3. 构建请求
    let _msg_count = full_messages.len();
    let req = ChatRequest {
        messages: full_messages,
        model: model_id,
        temperature: 0.8,
        max_tokens: None,
    };

    // 4. 创建 Provider 并获取流
    let provider = Provider::try_from(provider_config).map_err(|e| e.to_string())?;
    let mut stream = provider
        .stream_chat(req, abort_flag.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 5. 流式循环：转发给外部通道 + 收集完整回复
    let mut full_reply = String::new();

    // 检查是否被取消
    if abort_flag.load(Ordering::SeqCst) {
        tracing::debug!("[stream_chat] 检测到取消信号");
        if let Some(ref sender) = stream_sender {
            let _ = sender.send(LlmStreamEvent::Done);
        }
        return Ok(full_reply);
    }

    while let Some(item) = stream.next().await {
        match item {
            Ok(LlmStreamEvent::TextDelta { text }) => {
                full_reply.push_str(&text);
                if let Some(ref sender) = stream_sender {
                    let _ = sender.send(LlmStreamEvent::TextDelta { text });
                }
            }
            Ok(LlmStreamEvent::Done) => {
                tracing::debug!("[stream_chat] 收到 Done 事件, full_reply length={}", full_reply.len());
                if let Some(ref sender) = stream_sender {
                    let _ = sender.send(LlmStreamEvent::Done);
                }
            }
            Err(e) => {
                tracing::error!("[stream_chat] 流式处理出错: {}", e);
                return Err(e.to_string());
            }
        }
    }

    tracing::debug!("[stream_chat] 流式循环结束, full_reply length={}", full_reply.len());
    Ok(full_reply)
}

/// 消息类型别名，简化 API
pub type ChatMessage = LlmChatMessage;

/// 获取账户的模型配置（Provider + model_id）
pub async fn get_provider_config(
    cache: Arc<Cache>,
    db_state: &DbState,
    account_id: &str,
) -> Result<(ProviderConfigPayload, String), String> {
    get_account_model_config(cache, db_state, account_id).await
}

/// 统一聊天入口：保存消息、查历史、预占位、流式调用、更新占位
///
/// # 参数
/// - `db_state`: 数据库状态
/// - `ctx`: ChatContext 聊天上下文（含 account_id/chat_type/session_id/messages）
/// - `stream_sender`: 流式事件发送通道（可选），存在时转发 `LlmStreamEvent`
/// - `abort_flag`: 取消标记，用于中断流式响应
///
/// # 返回
/// 成功时返回 LLM 完整回复字符串，由调用方自行决定后续操作
pub async fn chat_with_placeholder(
    db_state: &DbState,
    ctx: ChatContext,
    stream_sender: Option<LlmStreamSender>,
    abort_flag: Arc<AtomicBool>,
) -> Result<String, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    // 获取全局 Cache 单例
    let _cache = Cache::get_global().map_err(|e| e.to_string())?;

    // 1. 保存当前用户消息到数据库（取最后一条 user 消息）
    if let Some(last_user_msg) = ctx.messages.iter().filter(|m| m.role == Role::User).last() {
        let user_payload = CreateMessagePayload {
            account_id: ctx.account_id.clone(),
            chat_type: ctx.chat_type.clone(),
            session_id: ctx.session_id.clone(),
            role: "user".to_string(),
            content: last_user_msg.content.clone(),
            parent_message_id: None,
            thinking: None,
            tool_calls: None,
            tool_call_id: None,
            tool_output: None,
            extends: Some("{}".to_string()),
            status: Some("completed".to_string()),
            metadata: Some("{}".to_string()),
        };
        if let Err(e) = chat::save_message(&*db, user_payload).await {
            eprintln!("[chat_with_placeholder] 保存用户消息失败: {}", e);
        }
    }

    // 2. 查询该账户+会话的历史消息（不区分 chat_type）
    let history = chat::get_messages(
        &*db,
        ctx.account_id.clone(),
        Some(ctx.session_id.clone()),
        None, // 不区分 chat_type
        Some(50),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    // 3. 将历史消息转换为 ChatMessage
    let full_messages: Vec<ChatMessage> = history
        .into_iter()
        .map(|msg| ChatMessage {
            role: match msg.role.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "system" => Role::System,
                _ => Role::User,
            },
            content: msg.content.unwrap_or_default(),
        })
        .collect();

    // 4. 预插入 assistant 占位消息（status=pending, content=空）
    let placeholder_payload = CreateMessagePayload {
        account_id: ctx.account_id.clone(),
        chat_type: ctx.chat_type.clone(),
        session_id: ctx.session_id.clone(),
        role: "assistant".to_string(),
        content: String::new(),
        parent_message_id: None,
        thinking: None,
        tool_calls: None,
        tool_call_id: None,
        tool_output: None,
        extends: Some("{}".to_string()),
        status: Some("pending".to_string()),
        metadata: Some("{}".to_string()),
    };
    let placeholder_id = match chat::save_message(&*db, placeholder_payload).await {
        Ok(msg) => {
            tracing::debug!("[chat_with_placeholder] 预插入 assistant 占位消息成功, placeholder_id: {}", msg.id);
            msg.id
        }
        Err(e) => {
            tracing::error!("[chat_with_placeholder] 预插入 assistant 占位消息失败: {}", e);
            String::new()
        }
    };

    tracing::debug!("[chat_with_placeholder] 开始调用 stream_chat, placeholder_id={}, messages count={}", placeholder_id, full_messages.len());

    // 5. 调用 stream_chat 执行流式聊天
    // 保存 abort_flag 克隆用于后续检测取消状态
    let abort_flag_for_check = abort_flag.clone();
    let result = stream_chat(
        db_state,
        &ctx.account_id,
        full_messages,
        None, // 统一入口不添加系统提示词，由调用方在 messages 中组装
        stream_sender,
        abort_flag,
    )
    .await;
    tracing::debug!("[chat_with_placeholder] stream_chat result: {:?}", result);
    // 6. 根据结果更新 assistant 占位消息
    let is_cancelled = abort_flag_for_check.load(Ordering::SeqCst);
    if !placeholder_id.is_empty() {
        let pid = placeholder_id.clone(); // 保存副本用于日志

        // 如果被取消且没有内容，则标记为 cancelled 状态并删除内容
        if is_cancelled && result.as_ref().map(|s| s.is_empty()).unwrap_or(false) {
            tracing::debug!("[chat_with_placeholder] LLM 调用被取消，准备删除占位消息, placeholder_id={}", pid);
            if let Err(e) = chat::delete_message(&*db, placeholder_id).await {
                tracing::error!("[chat_with_placeholder] 删除已取消的占位消息失败: {}", e);
            }
        } else {
            match &result {
                Ok(reply) => {
                    tracing::debug!("[chat_with_placeholder] LLM 调用成功，准备更新消息 status=completed, placeholder_id={}", pid);
                    if let Err(e) = chat::update_message(
                        &*db,
                        placeholder_id,
                        Some(reply.clone()),
                        Some("completed".to_string()),
                    )
                    .await
                    {
                        tracing::error!("[chat_with_placeholder] 更新 assistant 成功状态失败: {}", e);
                    } else {
                        tracing::debug!("[chat_with_placeholder] 消息状态已更新为 completed, placeholder_id={}", pid);
                    }
                }
                Err(e) => {
                    let error_content = format!("**调用失败**\n\n{}", e);
                    tracing::error!("[chat_with_placeholder] LLM 调用失败，准备更新消息 status=error, placeholder_id={}, error={}", placeholder_id, e);
                    if let Err(e) = chat::update_message(
                        &*db,
                        placeholder_id,
                        Some(error_content),
                        Some("error".to_string()),
                    )
                    .await
                    {
                        tracing::error!("[chat_with_placeholder] 更新 assistant 错误状态失败: {}", e);
                    }
                }
            }
        }
    } else {
        tracing::warn!("[chat_with_placeholder] placeholder_id 为空，跳过状态更新");
    }

    result
}