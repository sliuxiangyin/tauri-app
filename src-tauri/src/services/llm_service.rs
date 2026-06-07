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
    types::{ChatMessage as LlmChatMessage, ChatRequest, ProviderConfigPayload, Role},
    AgentConfig, AgentEventCallback, AgentResultSummary, AgentRunner, AgentStreamEvent,
    IntentAnalyzer, LlmProvider, LlmStreamEvent, LlmStreamSender, PlanExecutor, Provider,
};
use crate::services::chat_model_service::ChatModelService;
use crate::services::chat_tools_service;
use crate::services::llm::tool_executor::McpToolExecutor;
use crate::services::messages_service::MessagesService;
use crate::services::messages::MessagesSession;
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
        Self { db, cache, messages_service }
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
        let user_content = ctx.messages
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
        let history = self.messages_service
            .get_messages(
                ctx.account_id.clone(),
                Some(ctx.session_id.clone()),
                None,
                Some(50),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;

        // 3. 转换为 ChatMessage（从 blocks 中提取 content）
        let full_messages: Vec<ChatMessage> = history
            .into_iter()
            .map(|msg| {
                // 从 blocks 中提取第一个 Text 类型的 content
                let content = msg.blocks
                    .iter()
                    .filter(|b| b.block_type == "text")
                    .map(|b| b.content.clone().unwrap_or_default())
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
                let model_info = crate::services::db::chat_model::get_first_enabled_model(&*db_conn)
                    .await
                    .map_err(|e| e.to_string())?;
                let info = model_info.ok_or("no enabled model found")?;
                (info.payload, info.model_id)
            }
        };

        // 6. 创建 Provider
        let provider = Provider::try_from(provider_config.clone()).map_err(|e| e.to_string())?;
        let provider = match provider {
            Provider::OpenAiCompatible(p) => Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>,
            Provider::Anthropic(p) => Arc::new(p.with_model(model_id.clone())) as Arc<dyn LlmProvider>,
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
        let tools_ref = if tools.is_empty() { None } else { Some(tools.clone()) };

        // 8. 分析用户意图
        let intent_plan = analyze_intent(&provider, &model_id, &full_messages, &tools).await?;

        // 9. 执行路径
        let result = if intent_plan.need_agent && !intent_plan.steps.is_empty() {
            tracing::info!("[LlmService] 进入 Agent 模式");
            // Agent 模式
            self.execute_agent_mode(provider, mcp, tools, intent_plan, abort_flag).await
        } else {
            tracing::info!("[LlmService] 进入普通模式");
            // 普通模式
            self.execute_simple_mode(provider, model_id, full_messages, tools_ref, stream_sender, abort_flag).await
        };

        // 10. 完成会话（写入内容块 + 更新状态）
        match &result {
            Ok(reply) => {
                // 将回复写入 conversations 表的 Text block
                let _ = session.add_text_block(reply).await;
                session.complete(crate::services::messages::MessageStatus::Completed).await;
            }
            Err(e) => {
                let error_content = format!("**调用失败**\n\n{}", e);
                let _ = session.add_text_block(&error_content).await;
                session.complete(crate::services::messages::MessageStatus::Failed).await;
            }
        }

        result
    }

    /// 执行普通模式
    async fn execute_simple_mode(
        &self,
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

        let (results, _) = crate::provider::llm::process_tool_batch(
            stream,
            None,
            stream_sender.as_ref(),
        )
        .await
        .map_err(|e| e.to_string())?;

        Ok(results.into_iter().next().unwrap_or_default())
    }

    /// 执行 Agent 模式
    async fn execute_agent_mode(
        &self,
        provider: Arc<dyn LlmProvider>,
        mcp: Arc<dyn McpClient>,
        tools: Vec<crate::provider::llm::types::ToolDefinition>,
        intent_plan: crate::provider::llm::types::IntentPlan,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<String, String> {
        tracing::info!("[LlmService] 进入 Agent 模式，步骤数={}", intent_plan.steps.len());

        let executor = Arc::new(McpToolExecutor::new(mcp));
        let tool_names: Vec<String> = tools.iter().map(|t| t.function.name.clone()).collect();

        let plan_executor = PlanExecutor::new(executor)
            .with_available_tools(tool_names)
            .with_llm_provider(provider)
            .with_max_retries(2);

        match plan_executor.execute_plan(intent_plan, abort_flag).await {
            Ok(plan_result) => {
                tracing::info!(
                    "[LlmService] Plan 执行完成: {}/{}",
                    plan_result.completed_steps,
                    plan_result.total_steps
                );
                Ok(plan_result.final_reply)
            }
            Err(e) => {
                tracing::error!("[LlmService] Plan 执行失败: {}", e);
                Err(e.to_string())
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────
// 私有辅助函数
// ──────────────────────────────────────────────────────────────

/// 分析用户意图
async fn analyze_intent(
    provider: &Arc<dyn LlmProvider>,
    model_id: &str,
    messages: &[ChatMessage],
    available_tools: &[crate::provider::llm::types::ToolDefinition],
) -> Result<crate::provider::llm::types::IntentPlan, String> {
    let analyzer = IntentAnalyzer::new(provider.clone())
        .with_model(model_id.to_string());
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
    service.chat(mcp_manager.into_dyn(), ctx, stream_sender, abort_flag).await
}
