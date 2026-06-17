//! Task Planner Agent
//!
//! 使用 LLM 将用户请求分解为 TaskStage DAG。
//!
//! ## 使用方式
//!
//! ```ignore
//! let agent = TaskPlannerAgent::new(provider.clone())
//!     .with_model("gpt-4o".into())
//!     .with_available_domains(vec!["browser".into(), "file".into()])
//!     .with_user_request("在百度搜索AI新闻并提取前三条结果".into());
//!
//! // 非流式
//! let plan = agent.run().await?;
//!
//! // 流式
//! let StreamingResponse { text_stream, parse_future } =
//!     agent.run_streaming().await?;
//! ```

use std::sync::Arc;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::planner::agent_base::{run_llm, run_streaming_llm, StreamingResponse};
use crate::provider::llm::providers::provider_trait::LlmProvider;
use crate::provider::llm::types::{ChatMessage, Role};

use crate::provider::llm::planner::agent_base::{LlmAgent, LlmAgentBase};
use super::prompt::TASK_PLANNER_PROMPT;
use super::types::TaskPlan;

/// Task Planner Agent
#[derive(Clone)]
pub struct TaskPlannerAgent {
    /// 共享的 LLM Agent 基类
    base: LlmAgentBase,
    /// 可用领域列表（注入到 prompt {{AVAILABLE_DOMAINS}}）
    available_domains: Vec<String>,
    /// 对话上下文（注入到 prompt {{CONVERSATION_CONTEXT}}）
    conversation_context: String,
    /// 用户请求（作为 user message 发送给 LLM）
    user_request: String,
}

impl TaskPlannerAgent {
    /// 创建 TaskPlannerAgent（使用 provider 默认模型）
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            base: LlmAgentBase::new(provider),
            available_domains: Vec::new(),
            conversation_context: String::new(),
            user_request: String::new(),
        }
    }

    /// 设置默认模型
    pub fn with_model(self, model: String) -> Self {
        Self {
            base: self.base.with_model(model),
            ..self
        }
    }

    /// 设置温度参数
    pub fn with_temperature(self, temperature: f32) -> Self {
        Self {
            base: self.base.with_temperature(temperature),
            ..self
        }
    }

    /// 设置最大 tokens
    pub fn with_max_tokens(self, max_tokens: Option<u32>) -> Self {
        Self {
            base: self.base.with_max_tokens(max_tokens),
            ..self
        }
    }

    /// 设置可用领域列表
    ///
    /// 将注入到 prompt 的 `{{AVAILABLE_DOMAINS}}`，多个领域用逗号分隔。
    pub fn with_available_domains(self, domains: Vec<String>) -> Self {
        Self {
            base: self.base,
            available_domains: domains,
            ..self
        }
    }

    /// 设置对话上下文
    ///
    /// 将注入到 prompt 的 `{{CONVERSATION_CONTEXT}}`。
    pub fn with_conversation_context(self, context: String) -> Self {
        Self {
            base: self.base,
            conversation_context: context,
            ..self
        }
    }

    /// 设置用户请求
    ///
    /// 作为 user message 发送给 LLM，同时在 system prompt 的
    /// `{{CONVERSATION_CONTEXT}}` 之外提供主要输入。
    pub fn with_user_request(self, request: String) -> Self {
        Self {
            base: self.base,
            user_request: request,
            ..self
        }
    }

    /// 非流式执行：调用 LLM 并返回 TaskPlan
    pub async fn run(&self) -> Result<TaskPlan, LlmError> {
        let messages = self.build_messages();
        run_llm(&self.base, messages, |r| self.parse_response(r)).await
    }

    /// 流式执行：返回 text_stream + parse_future
    pub async fn run_streaming(&self) -> Result<StreamingResponse<TaskPlan>, LlmError> {
        let messages = self.build_messages();
        // parse_response 是 &self 方法，需要 Arc 包装以移入闭包
        let parse_fn = {
            let prompt_text = String::new(); // 不需要额外捕获
            move |response: &str| -> Result<TaskPlan, LlmError> {
                let json_str = Self::extract_json(response);
                serde_json::from_str(json_str).map_err(|e| {
                    LlmError::ParseError(format!(
                        "Failed to parse TaskPlan JSON: {}\n--- response ---\n{}",
                        e, response
                    ))
                })
            }
        };
        run_streaming_llm(&self.base, messages, parse_fn).await
    }
}

impl LlmAgent for TaskPlannerAgent {
    type Output = TaskPlan;

    fn base(&self) -> &LlmAgentBase {
        &self.base
    }

    fn build_messages(&self) -> Vec<ChatMessage> {
        let system_prompt = self.build_system_prompt();
        let user_message = format!("【用户请求】\n{}", self.user_request);

        vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, user_message),
        ]
    }

    fn parse_response(&self, response: &str) -> Result<TaskPlan, LlmError> {
        let json_str = Self::extract_json(response);
        serde_json::from_str(json_str).map_err(|e| {
            LlmError::ParseError(format!(
                "Failed to parse TaskPlan JSON: {}\n--- response ---\n{}",
                e, response
            ))
        })
    }
}

impl TaskPlannerAgent {
    /// 替换 prompt 中的模板变量
    fn build_system_prompt(&self) -> String {
        let domains_str = if self.available_domains.is_empty() {
            String::new()
        } else {
            self.available_domains.join(", ")
        };

        TASK_PLANNER_PROMPT
            .replace("{{AVAILABLE_DOMAINS}}", &domains_str)
            .replace("{{CONVERSATION_CONTEXT}}", &self.conversation_context)
    }

    /// 从 LLM 响应中提取 JSON 子串
    ///
    /// 优先匹配 Markdown 代码块（```json ... ```），其次取首尾 { } 切片。
    fn extract_json(response: &str) -> &str {
        let trimmed = response.trim();

        // 尝试匹配 ```json ... ``` 代码块
        if let (Some(start), Some(end)) = (trimmed.find("```json"), trimmed.rfind("```")) {
            if end > start + 7 {
                return &trimmed[start + 7..end].trim();
            }
        }

        // 尝试匹配首尾 { } 切片
        if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
            if end >= start {
                return &trimmed[start..=end];
            }
        }

        trimmed
    }
}
