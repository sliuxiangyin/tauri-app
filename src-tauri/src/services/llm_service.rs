//! LLM 服务层
//!
//! 职责：
//! - 统一 LLM 流式调用入口
//! - 流式输出通过外部传入的 `stream_sender` 转发，调用方统一处理 emit
//! - 返回完整回复供调用方使用
//! - 统一消息占位+状态机管理
//! - Agent 循环支持（多步工具调用）
//! - 意图识别与计划生成（Plan-Execute 模式）

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::{
    types::{ChatMessage as LlmChatMessage, ChatRequest, ProviderConfigPayload, Role},
    AgentConfig, AgentEventCallback, AgentResultSummary, AgentRunner, AgentStreamEvent,
    IntentAnalyzer, LlmProvider, LlmStreamEvent, LlmStreamSender, PlanExecutor, Provider,
};
use crate::provider::mcp::McpManager;
use crate::services::chat_model_service::get_account_model_config;
use crate::services::chat_tools_service;
use crate::services::db::chat::{self, CreateMessagePayload};
use crate::services::llm::tool_executor::McpToolExecutor;
use crate::types::chat::ChatContext;

/// 消息类型别名，简化 API
pub type ChatMessage = LlmChatMessage;

/// 构建聊天消息列表
fn build_messages(
    messages: Vec<ChatMessage>,
    system_prompt: Option<&str>,
) -> Vec<ChatMessage> {
    let mut full_messages = Vec::new();

    if let Some(prompt) = system_prompt {
        full_messages.push(ChatMessage::new(Role::System, prompt));
    }

    for msg in messages {
        full_messages.push(msg);
    }

    full_messages
}

/// 分析用户意图，返回是否需要 Agent 模式
async fn analyze_intent_for_chat(
    provider: Arc<dyn LlmProvider>,
    model_id: &str,
    messages: &[ChatMessage],
    available_tools: &[crate::provider::llm::types::ToolDefinition],
) -> Result<crate::provider::llm::types::IntentPlan, String> {
    let analyzer = IntentAnalyzer::new(provider)
        .with_model(model_id.to_string());
    analyzer
        .analyze(messages.to_vec(), available_tools.to_vec())
        .await
        .map_err(|e| e.to_string())
}

/// 流式聊天（普通模式）- 单次调用，不执行工具
pub async fn stream_chat_simple(
    provider: Arc<dyn LlmProvider>,
    model_id: String,
    messages: Vec<ChatMessage>,
    tools: Option<Vec<crate::provider::llm::types::ToolDefinition>>,
    stream_sender: Option<LlmStreamSender>,
    abort_flag: Arc<AtomicBool>,
) -> Result<String, String> {
    let req = ChatRequest {
        messages,
        model: model_id,
        temperature: 0.8,
        max_tokens: None,
        tools,
    };

    let stream = provider
        .stream_chat(req, abort_flag)
        .await
        .map_err(|e| e.to_string())?;

    let sender = stream_sender.clone();
    let (results, _) = crate::provider::llm::process_tool_batch(
        stream,
        None,
        sender.as_ref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    let reply = results.into_iter().next().unwrap_or_default();
    Ok(reply)
}

/// 流式聊天（Agent 模式）- 支持多步工具调用循环
///
/// 与 `stream_chat_simple()` 的区别：
/// - 自动循环执行工具调用，直到完成或达到终止条件
/// - 实时推送 AgentStreamEvent 事件（包含 LLM 事件透传 + Agent 状态事件）
/// - 支持最大步数、空响应阈值、超时等控制
///
/// # 返回
/// (完整回复, Agent 结果摘要)
pub async fn stream_chat_agent(
    provider: Arc<dyn LlmProvider>,
    messages: Vec<ChatMessage>,
    tools: Option<Vec<crate::provider::llm::types::ToolDefinition>>,
    tool_executor: Arc<dyn crate::provider::llm::agent::runner::AgentToolExecutor>,
    stream_sender: Option<LlmStreamSender>,
    abort_flag: Arc<AtomicBool>,
) -> Result<(String, AgentResultSummary), String> {
    // 克隆 stream_sender 供回调使用
    let stream_sender_for_callback = stream_sender.clone();

    // 创建事件回调闭包：直接发送事件到 stream_sender
    let callback: AgentEventCallback = Arc::new(move |event| {
        // LLM 事件透传：直接发送
        if let AgentStreamEvent::Llm(llm_event) = &event {
            if let Some(ref sender) = stream_sender_for_callback {
                let _ = sender.send(llm_event.clone());
            }
        }
        // Agent 状态事件：记录日志
        match &event {
            AgentStreamEvent::AgentStart { step } => {
                tracing::info!("[Agent] Started at step {}", step);
            }
            AgentStreamEvent::StepStart { step } => {
                tracing::debug!("[Agent] Step {} started", step);
            }
            AgentStreamEvent::StepComplete { step, had_tool_call, tool_call_count } => {
                tracing::debug!(
                    "[Agent] Step {} complete: had_tool_call={}, tool_call_count={}",
                    step,
                    had_tool_call,
                    tool_call_count
                );
            }
            AgentStreamEvent::ToolStart { call_id, name, arguments } => {
                tracing::debug!("[Agent] Tool {} started: id={}, args={:?}", name, call_id, arguments);
            }
            AgentStreamEvent::ToolComplete { call_id, name, duration_ms, success } => {
                tracing::debug!(
                    "[Agent] Tool {} complete: id={}, duration={}ms, success={}",
                    name,
                    call_id,
                    duration_ms,
                    success
                );
            }
            AgentStreamEvent::ToolError { call_id, name, error } => {
                tracing::error!("[Agent] Tool {} error: id={}, error={}", name, call_id, error);
            }
            AgentStreamEvent::AgentComplete { total_steps, stop_reason, final_content } => {
                tracing::info!(
                    "[Agent] Completed: steps={}, reason={:?}, content_len={}",
                    total_steps,
                    stop_reason,
                    final_content.as_ref().map(|s| s.len()).unwrap_or(0)
                );
            }
            AgentStreamEvent::Progress { step, max_steps, message } => {
                tracing::debug!("[Agent] Progress: step {}/{}, {}", step, max_steps, message);
            }
            _ => {}
        }
    });

    // 创建 Agent 配置
    let agent_config = AgentConfig::new()
        .with_max_steps(10)  // 最多 10 步
        .with_timeout_total(std::time::Duration::from_secs(300))  // 5 分钟总超时
        .with_empty_threshold(3)  // 连续 3 次空响应则终止
        .with_error_threshold(5);  // 连续 5 次错误则终止

    // 创建并运行 AgentRunner
    let runner = AgentRunner::new(provider, tool_executor, agent_config, messages)
        .with_event_callback(callback)
        .with_abort_flag(abort_flag.clone());

    let request = ChatRequest {
        messages: vec![],  // messages 已通过 runner 传入
        model: String::new(),
        temperature: 0.8,
        max_tokens: None,
        tools,
    };

    let result = runner.run_streaming(request).await;

    // 处理结果
    match result {
        Ok((final_messages, summary)) => {
            // 获取最终回复
            let final_reply = final_messages
                .last()
                .map(|m| m.content.clone())
                .unwrap_or_default();

            // 发送 Done 事件
            if let Some(ref sender) = stream_sender {
                let _ = sender.send(LlmStreamEvent::Done);
            }

            Ok((final_reply, summary))
        }
        Err(e) => {
            tracing::error!("[stream_chat_agent] Agent execution failed: {}", e);
            Err(e.to_string())
        }
    }
}

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
/// - `mcp_manager`: MCP 管理器（用于获取工具列表）
/// - `ctx`: ChatContext 聊天上下文（含 account_id/chat_type/session_id/messages）
/// - `stream_sender`: 流式事件发送通道（可选），存在时转发 `LlmStreamEvent`
/// - `abort_flag`: 取消标记，用于中断流式响应
///
/// # 返回
/// 成功时返回 LLM 完整回复字符串，由调用方自行决定后续操作
pub async fn chat_with_placeholder<M: Into<Arc<McpManager>>>(
    db_state: &DbState,
    mcp_manager: M,
    ctx: ChatContext,
    stream_sender: Option<LlmStreamSender>,
    abort_flag: Arc<AtomicBool>,
) -> Result<String, String> {
    let mcp_manager =  mcp_manager.into();
    let db = db_state.get().await.map_err(|e| e.to_string())?;

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
            tracing::error!("[chat_with_placeholder] 保存用户消息失败: {}", e);
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
            role: Role::from_str(&msg.role),
            content: msg.content.unwrap_or_default(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
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

    // 5. 获取全局 Cache 并获取模型配置
    let cache = Cache::get_global().map_err(|e| e.to_string())?;
    let (provider_config, model_id) =
        get_account_model_config(cache.clone(), db_state, &ctx.account_id).await?;

    // 6. 创建 Provider（带 model 字段）
    let provider = Provider::try_from(provider_config.clone()).map_err(|e| e.to_string())?;
    // 根据 Provider 类型设置 model
    let provider = match provider {
        Provider::OpenAiCompatible(p) => Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>,
        Provider::Anthropic(p) => Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>,
        Provider::Ollama(p) => Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>,
    };
    
    // 7. 获取启用的工具列表
    let tools = chat_tools_service::get_enabled_tools(
        &*mcp_manager,
        &cache,
        &ctx.account_id,
        &ctx.session_id,
    )
    .await;
    let tools_ref = if tools.is_empty() { None } else { Some(tools.clone()) };
    tracing::info!("[chat_with_placeholder] 获取启用的工具：{:?}", tools);
    // 8. 分析用户意图
    tracing::info!("[chat_with_placeholder] 分析用户意图...");
    let intent_plan = match analyze_intent_for_chat(provider.clone(), &model_id, &full_messages, &tools).await {
        Ok(plan) => {
            tracing::info!(
                "[chat_with_placeholder] 意图分析完成: need_agent={}, reasoning={}",
                plan.need_agent,
                plan.reasoning
            );
            plan
        }
        Err(e) => {
            tracing::warn!("[chat_with_placeholder] 意图分析失败，回退到普通模式: {}", e);
            crate::provider::llm::types::IntentPlan::simple()
        }
    };
    tracing::info!("[chat_with_placeholder] 意图分析完成: need_agent={}, reasoning={}，steps={:?}",
        intent_plan.need_agent, intent_plan.reasoning, intent_plan.steps);
    // 9. 根据意图决定执行路径
    let abort_flag_for_check = abort_flag.clone();
    let result = if intent_plan.need_agent && !intent_plan.steps.is_empty() {
        // Agent 模式：使用 PlanExecutor 执行
        tracing::info!(
            "[chat_with_placeholder] 进入 Agent 模式，步骤数={}",
            intent_plan.steps.len()
        );

        // 创建工具执行器（同时实现 ToolExecutor 和 AgentToolExecutor）
        let executor = Arc::new(McpToolExecutor::new(mcp_manager.clone()));

        // 使用工具列表验证
        let tool_names: Vec<String> = tools.iter().map(|t| t.function.name.clone()).collect();

        // 创建 PlanExecutor（配置 LLM Provider 用于步骤失败时的分析决策）
        let plan_executor = PlanExecutor::new(executor.clone())
            .with_available_tools(tool_names)
            .with_llm_provider(provider.clone())
            .with_max_retries(2);

        // 执行计划
        match plan_executor.execute_plan(intent_plan, abort_flag.clone()).await {
            Ok(plan_result) => {
                tracing::info!(
                    "[chat_with_placeholder] Plan 执行完成: completed_steps={}/{}",
                    plan_result.completed_steps,
                    plan_result.total_steps
                );
                Ok(plan_result.final_reply)
            }
            Err(e) => {
                tracing::error!("[chat_with_placeholder] Plan 执行失败: {}", e);
                Err(e.to_string())
            }
        }
    } else {
        // 普通模式：直接流式聊天
        tracing::info!("[chat_with_placeholder] 进入普通模式");
        stream_chat_simple(
            provider,
            model_id,
            full_messages,
            tools_ref,
            stream_sender,
            abort_flag,
        )
        .await
    };

    // 7. 根据结果更新 assistant 占位消息
    let is_cancelled = abort_flag_for_check.load(Ordering::SeqCst);
    if !placeholder_id.is_empty() {
        let pid = placeholder_id.clone(); // 保存副本用于日志

        // 如果被取消且没有内容，则标记为 cancelled 状态并删除内容
        if is_cancelled && result.as_ref().map(|s| s.is_empty()).unwrap_or(false) {
            if let Err(e) = chat::delete_message(&*db, placeholder_id).await {
                tracing::error!("[chat_with_placeholder] 删除已取消的占位消息失败: {}", e);
            }
        } else {
            match &result {
                Ok(reply) => {
                    tracing::debug!(" LLM 调用成功，准备更新消息 status=completed, placeholder_id={}", pid);
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