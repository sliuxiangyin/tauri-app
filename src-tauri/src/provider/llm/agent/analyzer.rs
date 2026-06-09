//! 意图分析器
//!
//! 封装 LLM 意图分析能力：
//! - `analyze()`: 意图分析，使用 send_message（返回 IntentPlan）
//! - `decision_raw()`: 决策分析，使用 stream_chat（累积文本后返回）
//!
//! 设计原则：
//! - 意图分析属于 Agent 职责，不应放在 Provider 层
//! - 结构化响应（IntentPlan）用 send_message，简单直接
//! - 探索性决策/失败处理用 stream_chat，流式反馈给用户

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use futures_util::StreamExt;
use tracing::{debug, warn};

use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::prompts::intent_prompt::{
    build_intent_user_message, extract_user_request, intent_system_prompt, parse_intent_response,
};
use crate::provider::llm::providers::provider_trait::{LlmProvider, LlmStream};
use crate::provider::llm::types::{ChatMessage, ChatRequest, IntentPlan, Role, ToolDefinition};

/// 意图分析器
///
/// 使用示例：
/// ```ignore
/// let analyzer = IntentAnalyzer::new(provider.clone());
/// 
/// // 意图分析
/// let plan = analyzer.analyze(messages, tools).await?;
/// 
/// // 决策分析（流式）
/// let response = analyzer.decision_raw(messages, tools).await?;
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

    /// 分析用户意图并生成执行计划
    ///
    /// 使用 send_message，因为只需最终 IntentPlan JSON，无需流式反馈
    ///
    /// # 参数
    /// - `messages`: 对话消息列表
    /// - `available_tools`: 可用工具列表
    ///
    /// # 返回
    /// IntentPlan，包含是否需要 Agent 模式和执行步骤
    pub async fn analyze(
        &self,
        messages: Vec<ChatMessage>,
        available_tools: Vec<ToolDefinition>,
    ) -> Result<IntentPlan, LlmError> {
        
        let model = self.get_model()?;
        
        let response = self.send_intent_request(model, messages, available_tools).await?;
        
        parse_intent_response(&response)
    }

    /// 分析用户意图并返回原始响应
    ///
    /// 用于需要解析文本的场景（如探索性步骤决策）
    pub async fn analyze_raw(
        &self,
        messages: Vec<ChatMessage>,
        available_tools: Vec<ToolDefinition>,
    ) -> Result<String, LlmError> {
        let model = self.get_model()?;
        self.send_intent_request(model, messages, available_tools).await
    }

    /// 决策分析（使用 stream_chat）
    ///
    /// 用于探索性步骤决定工具、步骤失败处理等场景
    /// 返回累积的完整文本（而非 IntentPlan）
    pub async fn decision_raw(
        &self,
        messages: Vec<ChatMessage>,
        available_tools: Vec<ToolDefinition>,
    ) -> Result<String, LlmError> {
        let model = self.get_model()?;
        let req = self.build_intent_request(model, messages, available_tools)?;
        let stream = self.provider.stream_chat(req, Arc::new(AtomicBool::new(false))).await?;
        self.collect_stream_text(stream).await
    }

    /// 发送意图分析请求
    async fn send_intent_request(
        &self,
        model: String,
        messages: Vec<ChatMessage>,
        available_tools: Vec<ToolDefinition>,
    ) -> Result<String, LlmError> {
        let req = self.build_intent_request(model, messages, available_tools)?;
        self.provider.send_message(req).await
    }

    /// 构建意图分析请求
    fn build_intent_request(
        &self,
        model: String,
        messages: Vec<ChatMessage>,
        available_tools: Vec<ToolDefinition>,
    ) -> Result<ChatRequest, LlmError> {
        let system_prompt = intent_system_prompt();
        
        let user_request = extract_user_request(&messages);
        
        let user_message = build_intent_user_message(&available_tools, &user_request);
        
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

    /// 累积流式文本
    async fn collect_stream_text(&self, stream: LlmStream) -> Result<String, LlmError> {
        let mut text = String::new();
        futures_util::pin_mut!(stream);

        while let Some(item) = stream.next().await {
            match item {
                Ok(event) => match event {
                    LlmStreamEvent::TextDelta { text: delta } => {
                        debug!("IntentAnalyzer: received delta: {} chars", delta.len());
                        text.push_str(&delta);
                    }
                    LlmStreamEvent::Done => {
                        debug!("IntentAnalyzer: stream done, total {} chars", text.len());
                        break;
                    }
                    LlmStreamEvent::Error { code, message } => {
                        warn!("IntentAnalyzer: stream error: {} - {}", code, message);
                        return Err(LlmError::HttpStatus {
                            status: 0,
                            body: format!("Stream error: {} - {}", code, message),
                        });
                    }
                    // ToolCall 相关事件在意图分析中不应出现
                    LlmStreamEvent::ToolCallStart { .. } => {
                        warn!("IntentAnalyzer: unexpected ToolCallStart in intent analysis");
                    }
                    LlmStreamEvent::ToolCallDelta { .. } => {}
                    LlmStreamEvent::ToolCallDone { .. } => {}
                    LlmStreamEvent::ToolResult { .. } => {}
                    LlmStreamEvent::ReasoningDelta { .. } => {}
                    LlmStreamEvent::Reference { .. } => {}
                    LlmStreamEvent::AudioDelta { .. } => {}
                    LlmStreamEvent::Warning { .. } => {}
                    LlmStreamEvent::Usage { .. } => {}
                    LlmStreamEvent::Metadata { .. } => {}
                    LlmStreamEvent::PlanStart { .. } => {}
                    LlmStreamEvent::PlanUpdate { .. } => {}
                    LlmStreamEvent::BlockStart { .. } => {}
                    LlmStreamEvent::PlanSteps { .. } => {},
                },
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if text.is_empty() {
            return Err(LlmError::EmptyResponse);
        }

        Ok(text)
    }
}

/// 为特定 Provider 创建 IntentAnalyzer 的辅助函数
///
/// 由于不同 Provider 对 system 消息的处理不同，提供此辅助函数
pub mod provider_helper {
    use super::*;

    /// 为不支持 system role 的 Provider（如 Anthropic）构建请求
    pub fn build_anthropic_style_request(
        model: &str,
        messages: &[ChatMessage],
        available_tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: Option<u32>,
    ) -> Result<ChatRequest, LlmError> {
        let system_prompt = intent_system_prompt();
        let user_request = extract_user_request(messages);
        let user_message = build_intent_user_message(available_tools, &user_request);

        // Anthropic 不支持 System 消息作为独立 role，需要合并到 user 消息
        let req_messages = vec![ChatMessage::new(
            Role::User,
            &format!("{}\n\n{}", system_prompt, user_message),
        )];

        Ok(ChatRequest {
            messages: req_messages,
            model: model.to_string(),
            temperature,
            max_tokens,
            tools: None,
        })
    }

    /// 为支持 system role 的 Provider（如 OpenAI、Ollama）构建请求
    pub fn build_request(
        model: &str,
        messages: &[ChatMessage],
        available_tools: &[ToolDefinition],
        temperature: f32,
        max_tokens: Option<u32>,
    ) -> Result<ChatRequest, LlmError> {
        let system_prompt = intent_system_prompt();
        let user_request = extract_user_request(messages);
        let user_message = build_intent_user_message(available_tools, &user_request);

        let req_messages = vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, &user_message),
        ];

        Ok(ChatRequest {
            messages: req_messages,
            model: model.to_string(),
            temperature,
            max_tokens,
            tools: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyzer_builder() {
        let analyzer = IntentAnalyzer::new(Arc::new(crate::provider::llm::providers::OpenAiCompatible::new(
            "http://localhost".to_string(),
            "test".to_string(),
        )))
        .with_model("gpt-4".to_string())
        .with_temperature(0.2)
        .with_max_tokens(Some(4096));

        assert_eq!(analyzer.model, "gpt-4");
        assert_eq!(analyzer.temperature, 0.2);
        assert_eq!(analyzer.max_tokens, Some(4096));
    }

    #[test]
    fn test_provider_helpers() {
        let messages = vec![
            ChatMessage::new(Role::System, "system"),
            ChatMessage::new(Role::User, "hello"),
        ];
        let tools: Vec<ToolDefinition> = vec![];

        // Test OpenAI style
        let req = provider_helper::build_request("gpt-4", &messages, &tools, 0.1, Some(2048)).unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, Role::System);
        assert_eq!(req.messages[1].role, Role::User);

        // Test Anthropic style
        let req = provider_helper::build_anthropic_style_request("claude-3", &messages, &tools, 0.1, Some(2048)).unwrap();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, Role::User);
        assert!(req.messages[0].content.contains("system"));
    }
}
