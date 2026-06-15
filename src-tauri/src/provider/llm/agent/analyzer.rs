//! 意图分析器
//!
//! 封装 LLM 意图分析能力：
//! - `analyze()`: 意图分析，使用 send_message（返回 [`IntentResponse`]）
//!
//! 设计原则：
//! - 意图分析属于 Agent 职责，不应放在 Provider 层
//! - 本模块**只做意图判断**（`need_agent` + `reasoning`），不生成步骤
//! - 步骤规划（Plan）和探索性决策由独立的 Plan / Executor 模块负责
//! - 意图请求不携带工具上下文（`tools: None`），工具选择属于 Plan / 执行阶段

use std::sync::Arc;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::prompts::intent_prompt::{
    build_intent_user_message, extract_user_request, intent_system_prompt, parse_intent_response,
    IntentResponse,
};
use crate::provider::llm::providers::provider_trait::LlmProvider;
use crate::provider::llm::types::{ChatMessage, ChatRequest, Role};

/// 意图分析器
///
/// 使用示例：
/// ```ignore
/// let analyzer = IntentAnalyzer::new(provider.clone());
///
/// // 意图分析
/// let response = analyzer.analyze(messages).await?;
/// if response.need_agent {
///     // 将 response.reasoning 喂给独立的 Plan 生成模块
/// }
/// ```
pub struct IntentAnalyzer {
    /// LLM Provider
    provider: Arc<dyn LlmProvider>,
    /// 默认模型
    model: String,
    /// 温度参数
    temperature: f32,
    /// 最大 tokens
    max_tokens: Option<u32>,
}

impl IntentAnalyzer {
    /// 创建新的意图分析器
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            model: String::new(),
            temperature: 0.1,
            max_tokens: Some(2048),
        }
    }

    /// 设置默认模型
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// 设置温度参数
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// 设置最大 tokens
    pub fn with_max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// 分析用户意图
    ///
    /// 使用 send_message，因为只需最终 [`IntentResponse`] JSON，无需流式反馈。
    /// 意图请求**不携带工具上下文**（`tools: None`），工具选择属于 Plan / 执行阶段。
    ///
    /// # 参数
    /// - `messages`: 对话消息列表
    ///
    /// # 返回
    /// [`IntentResponse`]，包含 `need_agent`（是否需要 Agent 模式）
    /// 和 `reasoning`（任务分解描述，可用于后续 Plan 生成）。
    pub async fn analyze(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<IntentResponse, LlmError> {
        let model = self.get_model()?;
        let response = self.send_intent_request(model, messages).await?;
        parse_intent_response(&response)
    }

    /// 发送意图分析请求
    async fn send_intent_request(
        &self,
        model: String,
        messages: Vec<ChatMessage>,
    ) -> Result<String, LlmError> {
        let req = self.build_intent_request(model, messages)?;
        self.provider.send_message(req).await
    }

    /// 构建意图分析请求
    ///
    /// 注意：意图阶段不携带工具列表（`tools: None`），
    /// 工具选择属于 Plan / 执行阶段。
    fn build_intent_request(
        &self,
        model: String,
        messages: Vec<ChatMessage>,
    ) -> Result<ChatRequest, LlmError> {
        let system_prompt = intent_system_prompt();
        let user_request = extract_user_request(&messages);
        let user_message = build_intent_user_message(&user_request);

        let req_messages = vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, &user_message),
        ];

        Ok(ChatRequest {
            messages: req_messages,
            model,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            tools: None,
        })
    }

    /// 获取模型（支持外部传入或默认模型）
    fn get_model(&self) -> Result<String, LlmError> {
        if self.model.is_empty() {
            self.provider
                .default_model()
                .map(|m| m.to_string())
                .ok_or_else(|| LlmError::Config("Model not set for intent analysis".to_string()))
        } else {
            Ok(self.model.clone())
        }
    }
}

