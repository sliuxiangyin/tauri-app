//! LLM 服务层
//!
//! 职责：
//! - 统一 LLM 流式调用入口
//! - 流式输出通过外部传入的 `stream_sender` 转发
//! - 返回完整回复供调用方使用
//! - 统一消息占位+状态机管理
//! - Agent 循环支持（多步工具调用）
//! - 意图识别与计划生成（Plan-Execute 模式）
//!
//! ## 依赖注入
//!
//! 通过构造器模式注入依赖：
//! - `DbAccessor`: 数据库访问
//! - `Cache`: 缓存管理器

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::provider::cache::Cache;
use crate::provider::llm::{
    types::{ChatMessage as LlmChatMessage, ChatRequest, Role},
    IntentAnalyzer, LlmProvider, LlmStreamSender, LlmStreamEvent, PlanExecutor, Provider,
};
use crate::services::chat_model_service::ChatModelService;
use crate::services::chat_tools_service;
use crate::provider::llm::block_sender::BlockSender;
use crate::provider::llm::llm_tool_trait::ToolExecutor;
use crate::services::llm::tool_executor::McpToolExecutor;
use crate::services::messages::MessagesSession;
use crate::services::messages_service::MessagesService;
use crate::services::traits::{DbAccessor, McpClient};
use crate::types::chat::ChatContext;

/// 消息类型别名，简化 API
pub type ChatMessage = LlmChatMessage;

// ──────────────────────────────────────────────────────────────
// LLM Service（面向对象 + 依赖注入）
// ──────────────────────────────────────────────────────────────

/// LLM 服务
///
/// 通过构造器注入依赖，支持可测试化
pub struct LlmService {
    db: Arc<dyn DbAccessor>,
    cache: Arc<Cache>,
    messages_service: MessagesService,
}

impl LlmService {
    /// 创建新的 LLM 服务
    pub fn new(db: Arc<dyn DbAccessor>, cache: Arc<Cache>) -> Self {
        let messages_service = MessagesService::new(db.clone());
        Self {
            db,
            cache,
            messages_service,
        }
    }

    /// 统一聊天入口：保存消息、查历史、预占位、流式调用、更新占位
    ///
    /// # 参数
    /// - `mcp`: MCP 客户端（Trait 注入）
    /// - `ctx`: ChatContext 聊天上下文
    /// - `stream_sender`: 流式事件发送通道
    /// - `abort_flag`: 取消标记
    pub async fn chat<M: Into<Arc<dyn McpClient>>>(
        &self,
        mcp: M,
        ctx: ChatContext,
        stream_sender: Option<LlmStreamSender>,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<String, String> {
        let mcp = mcp.into();
        let db = self.db.get().await.map_err(|e| e.to_string())?;

        // 0. 获取用户消息内容
        let user_content = ctx
            .messages
            .iter()
            .filter(|m| m.role == Role::User)
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // 1. 使用 MessagesSession 管理会话（内部创建 user + assistant 占位消息）
        let mut session = MessagesSession::new(
            ctx.account_id.clone(),
            ctx.chat_type.clone(),
            ctx.session_id.clone(),
            user_content,
            db.clone(),
        )
        .await
        .map_err(|e| e.to_string())?;

        // 2. 查询历史消息（此时会包含刚创建的 user 消息）
        let history = self
            .messages_service
            .get_messages(
                ctx.account_id.clone(),
                Some(ctx.session_id.clone()),
                None,
                Some(50),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;

        // 3. 转换为 ChatMessage（从 content 中提取文本）
        let full_messages: Vec<ChatMessage> = history
            .into_iter()
            .map(|msg| {
                // 从统一 content 序列中提取 Text 类型块的内容
                let content = msg
                    .content
                    .iter()
                    .filter_map(|item| match item {
                        crate::services::db::message::ContentItem::Block(b)
                            if b.block_type == "text" =>
                        {
                            Some(b.content.clone().unwrap_or_default())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                ChatMessage {
                    role: Role::from_str(&msg.role),
                    content,
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                }
            })
            .collect();

        // 4. 获取模型配置
        let model_service = ChatModelService::new(self.cache.clone());
        let (provider_config, model_id) = match model_service.get_account_model(&ctx.account_id) {
            Ok(Some(selection)) => {
                let db_conn = self.db.get().await.map_err(|e| e.to_string())?;
                let model_info = crate::services::db::chat_model::get_model_by_ids(
                    &*db_conn,
                    &selection.config_id,
                    &selection.model_id,
                )
                .await
                .map_err(|e| e.to_string())?;
                match model_info {
                    Some(info) => (info.payload, selection.model_id),
                    None => {
                        return Err("saved model not found".to_string());
                    }
                }
            }
            _ => {
                // 未选择，返回第一个开启的模型
                let db_conn = self.db.get().await.map_err(|e| e.to_string())?;
                let model_info =
                    crate::services::db::chat_model::get_first_enabled_model(&*db_conn)
                        .await
                        .map_err(|e| e.to_string())?;
                let info = model_info.ok_or("no enabled model found")?;
                (info.payload, info.model_id)
            }
        };

        // 6. 创建 Provider
        let provider = Provider::try_from(provider_config.clone()).map_err(|e| e.to_string())?;
        let provider = match provider {
            Provider::OpenAiCompatible(p) => {
                Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>
            }
            Provider::Anthropic(p) => {
                Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>
            }
            Provider::Ollama(p) => Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>,
        };

        // 7. 获取启用的工具列表
        let tools = chat_tools_service::get_enabled_tools_from_client(
            &*mcp,
            &self.cache,
            &ctx.account_id,
            &ctx.session_id,
        )
        .await;
        let tools_ref = if tools.is_empty() {
            None
        } else {
            Some(tools.clone())
        };

        // 8. 分析用户意图
        let intent_plan = analyze_intent(&provider, &model_id, &full_messages, &tools).await?;

        // 9. 分发执行（plan 操作 + text block 写入在各 execute 方法内部完成）
        // 保存 stream_sender 的 clone 用于错误处理
        let _stream_sender_for_error = stream_sender.clone();

        let result = if intent_plan.need_agent && !intent_plan.steps.is_empty() {
            self.execute_agent_mode(
                &mut session,
                provider,
                mcp,
                tools,
                intent_plan,
                stream_sender,
                abort_flag,
            )
            .await
        } else {
            tracing::info!("[LlmService] 进入普通模式");
            self.execute_simple_mode(
                &mut session,
                provider,
                 mcp,
                model_id,
                full_messages,
                tools_ref,
                stream_sender,
                abort_flag,
            )
            .await
        };

        // 10. 统一完成会话
        match &result {
            Ok(_) => {
                session
                    .complete(crate::services::messages::MessageStatus::Completed)
                    .await
            }
            Err(e) => {
                let error_content = format!("**调用失败**\n\n{}", e);
                let _block_info = session.add_text_block(&error_content).await;
                session
                    .complete(crate::services::messages::MessageStatus::Failed)
                    .await;
            }
        }

        result
    }

    /// 执行普通模式
    ///
    /// 负责：执行 LLM 流式调用 → 写入 text block
    async fn execute_simple_mode(
        &self,
        session: &mut MessagesSession,
        provider: Arc<dyn LlmProvider>,
           mcp: Arc<dyn McpClient>,
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
        // tracing::debug!("[LlmService] 调用 LLM 模型: {:?}", req);
        let stream = provider
            .stream_chat(req, abort_flag)
            .await
            .map_err(|e| e.to_string())?;
        let executor: Arc<dyn ToolExecutor> = Arc::new(McpToolExecutor::new(mcp.clone()));
        let result = crate::provider::llm::ordinary::process_tool_batch(stream, Some(executor), stream_sender.as_ref())
            .await
            .map_err(|e| e.to_string())?;

        // 保存文本块
        session.add_text_block(&result.text).await;

        // 保存工具调用记录（统一 block，包含参数和结果）
        for tool_call in &result.tool_calls {
            session.add_tool(tool_call).await;
        }

        Ok(result.text)
    }

    /// 执行 Agent 模式
    ///
    /// 负责：保存 Plan → 推送步骤列表 → 执行 Agent 循环 → 更新 Plan 结果 → 写入 text block
    async fn execute_agent_mode(
        &self,
        session: &mut MessagesSession,
        provider: Arc<dyn LlmProvider>,
        mcp: Arc<dyn McpClient>,
        tools: Vec<crate::provider::llm::types::ToolDefinition>,
        intent_plan: crate::provider::llm::types::IntentPlan,
        stream_sender: Option<LlmStreamSender>,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<String, String> {
        tracing::info!(
            "[LlmService] 进入 Agent 模式，步骤数={}",
            intent_plan.steps.len()
        );

        let stream_sender = stream_sender
            .ok_or_else(|| "stream_sender is required for agent mode".to_string())?;
        let mut block_sender = BlockSender::new(Some(stream_sender.clone()));
        // let mut block_sender = BlockSender::new(stream_sender.cloned());
        // block_sender.send("text");
        // 1. 保存 Plan
        let plan_id = match session.save_plan(&intent_plan).await {
            Ok(id) => {
                tracing::info!("[LlmService] 保存 Plan 成功: plan_id={}", id);
                Some(id)
            }
            Err(e) => {
                tracing::error!("[LlmService] 保存 Plan 失败: {}", e);
                None
            }
        };
      
        // 2. 推送 Plan 步骤列表给前端
        let _ = stream_sender.send(crate::provider::llm::llm_event::LlmStreamEvent::PlanSteps {
            reasoning: intent_plan.reasoning.clone(),
            steps: intent_plan.steps.clone(),
        });

        // TODO: 临时中断，方便前端测试 Plan 渲染 —— 恢复 Agent 执行时删除此段
        {
            let reply = format!(
                "**Agent 模式（已暂停）**\n\n{}\n\n共 {} 个步骤，暂不执行。",
                intent_plan.reasoning,
                intent_plan.steps.len()
            );

            let _ = stream_sender.send(crate::provider::llm::llm_event::LlmStreamEvent::TextDelta {
                text: reply.clone(),
            });

            let block_info = session.add_text_block(&reply).await;
            // 发送 BlockStart 事件给前端
            let _ = stream_sender.send(LlmStreamEvent::BlockStart {
                block_type: block_info.block_type,
                order_num: block_info.order_num,
            });
            return Ok(reply);
        }

        // 3. 执行 Agent 循环
        let executor: Arc<dyn ToolExecutor> = Arc::new(McpToolExecutor::new(mcp));
        let tool_names: Vec<String> = tools.iter().map(|t| t.function.name.clone()).collect();

        let plan_executor = PlanExecutor::new(executor)
            .with_available_tools(tool_names)
            .with_llm_provider(provider)
            .with_max_retries(2);

        let plan_result = plan_executor
            .execute_plan(intent_plan, abort_flag)
            .await
            .map_err(|e| e.to_string());

        // 3. 更新 Plan 结果
        if let Some(plan_id) = &plan_id {
            match &plan_result {
                Ok(pr) => {
                    tracing::info!(
                        "[LlmService] Plan 执行完成: {}/{}, plan_id={}",
                        pr.completed_steps,
                        pr.total_steps,
                        plan_id
                    );
                    let stop_reason_str = plan_stop_reason_to_str(&pr.stop_reason);
                    if let Err(e) = session
                        .update_plan_result(
                            serde_json::to_string(&pr.step_results).ok(),
                            &stop_reason_str,
                        )
                        .await
                    {
                        tracing::error!("[LlmService] 更新 Plan 结果失败: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("[LlmService] Plan 执行失败: {}, plan_id={}", e, plan_id);
                    if let Err(e) = session.update_plan_result(None, "failed").await {
                        tracing::error!("[LlmService] 更新 Plan 失败状态失败: {}", e);
                    }
                }
            }
        }

        // 4. 写入 text block 并发送 BlockStart 事件
        match &plan_result {
            Ok(pr) => {
                let block_info = session.add_text_block(&pr.final_reply).await;
                let _ = stream_sender.send(LlmStreamEvent::BlockStart {
                    block_type: block_info.block_type,
                    order_num: block_info.order_num,
                });
                Ok(pr.final_reply.clone())
            }
            Err(e) => Err(e.clone()),
        }
    }
}

// ──────────────────────────────────────────────────────────────
// 私有辅助函数
// ──────────────────────────────────────────────────────────────

/// 将 PlanStopReason 转换为字符串
fn plan_stop_reason_to_str(
    reason: &crate::provider::llm::agent::plan_executor::PlanStopReason,
) -> String {
    match reason {
        crate::provider::llm::agent::plan_executor::PlanStopReason::Completed => {
            "completed".to_string()
        }
        crate::provider::llm::agent::plan_executor::PlanStopReason::NoFinalReply => {
            "no_final_reply".to_string()
        }
        crate::provider::llm::agent::plan_executor::PlanStopReason::PartialFailure => {
            "partial_failure".to_string()
        }
        crate::provider::llm::agent::plan_executor::PlanStopReason::UserAbort => {
            "user_abort".to_string()
        }
        crate::provider::llm::agent::plan_executor::PlanStopReason::DependencyFailed => {
            "dependency_failed".to_string()
        }
        crate::provider::llm::agent::plan_executor::PlanStopReason::ToolNotFound => {
            "tool_not_found".to_string()
        }
    }
}

/// 分析用户意图
async fn analyze_intent(
    provider: &Arc<dyn LlmProvider>,
    model_id: &str,
    messages: &[ChatMessage],
    available_tools: &[crate::provider::llm::types::ToolDefinition],
) -> Result<crate::provider::llm::types::IntentPlan, String> {
    let analyzer = IntentAnalyzer::new(provider.clone()).with_model(model_id.to_string());
    analyzer
        .analyze(messages.to_vec(), available_tools.to_vec())
        .await
        .map_err(|e| e.to_string())
}

// ──────────────────────────────────────────────────────────────
// 兼容层（保留旧函数签名，逐步迁移）
// ──────────────────────────────────────────────────────────────

use crate::db::DbState;
use crate::provider::mcp::McpManager;

/// 统一聊天入口（兼容版）
///
/// 保留旧签名以兼容现有调用方，逐步迁移到 LlmService
pub async fn chat_with_placeholder(
    db: &DbState,
    mcp_manager: Arc<McpManager>,
    ctx: ChatContext,
    stream_sender: Option<LlmStreamSender>,
    abort_flag: Arc<AtomicBool>,
) -> Result<String, String> {
    let cache = Cache::get_global().map_err(|e| e.to_string())?;
    let service = LlmService::new(Arc::new(db.clone()), cache);
    service
        .chat(mcp_manager.into_dyn(), ctx, stream_sender, abort_flag)
        .await
}
