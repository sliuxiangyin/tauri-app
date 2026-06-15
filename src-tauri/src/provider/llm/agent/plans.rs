//! 计划生成器
//!
//! 封装 LLM 计划生成能力：
//! - `generate()`: 接收意图分析阶段的 content，生成执行计划（[`PlansResponse`]）
//!
//! 设计原则：
//! - 计划生成属于 Agent 职责，不应放在 Provider 层
//! - 本模块**只做计划生成**（steps 列表），不执行步骤
//! - 步骤执行由独立的 PlanExecutor 模块负责
//! - 计划请求不携带工具上下文（`tools: None`），工具选择属于执行阶段

use std::sync::Arc;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::prompts::plans_prompt::{
    build_plans_user_message, parse_plans_response, plans_system_prompt, PlansResponse,
};
use crate::provider::llm::providers::provider_trait::LlmProvider;
use crate::provider::llm::types::{ChatMessage, ChatRequest, Role, ToolDefinition};

/// 计划生成器
///
/// 使用示例：
/// ```ignore
/// let analyzer = PlansAnalyzer::new(provider.clone());
///
/// // 计划生成
/// let response = analyzer.generate(content).await?;
/// for step in response.steps {
///     // 将 step 喂给独立的 PlanExecutor 执行
/// }
/// ```
pub struct PlansAnalyzer {
    /// LLM Provider
    provider: Arc<dyn LlmProvider>,
    /// 默认模型
    model: String,
    /// 温度参数
    temperature: f32,
    /// 最大 tokens
    max_tokens: Option<u32>,
    /// 可用工具列表（传递给 LLM 作为上下文，非 ChatRequest.tools）
    tools: Vec<ToolDefinition>,
}

impl PlansAnalyzer {
    /// 创建新的计划生成器
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            model: String::new(),
            temperature: 0.1,
            max_tokens: Some(4096),
            tools: Vec::new(),
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

    /// 设置可用工具列表（用于在 user 消息中展示工具信息）
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// 生成执行计划
    ///
    /// 使用 send_message，因为只需最终 [`PlansResponse`] JSON，无需流式反馈。
    /// 计划请求**不携带工具上下文**（`tools: None`），工具选择属于执行阶段。
    ///
    /// # 参数
    /// - `content`: 包含用户请求与意图分析 reasoning 的完整文本
    ///
    /// # 返回
    /// [`PlansResponse`]，包含 `steps` 列表（PlanStep 数组）。
    pub async fn generate(&self, content: &str) -> Result<PlansResponse, LlmError> {
        let model = self.get_model()?;
        let response = self.send_plan_request(model, content).await?;
        parse_plans_response(&response)
    }
 
    /// 发送计划生成请求
    async fn send_plan_request(
        &self,
        model: String,
        content: &str,
    ) -> Result<String, LlmError> {
        let req = self.build_plan_request(model, content)?;
        self.provider.send_message(req).await
    }

    /// 构建计划生成请求
    ///
    /// 注意：计划阶段不携带工具列表（`tools: None`），
    /// 工具选择属于执行阶段。工具信息通过 user 消息尾部追加文本传递给 LLM。
    fn build_plan_request(
        &self,
        model: String,
        content: &str,
    ) -> Result<ChatRequest, LlmError> {
        let system_prompt = plans_system_prompt();
        let user_message = build_plans_user_message(content, &self.tools);
        tracing::debug!("User Message: {}", user_message);
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
                .ok_or_else(|| LlmError::Config("Model not set for plan generation".to_string()))
        } else {
            Ok(self.model.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plans_analyzer_builder_chain() {
        // 测试 builder 方法链式调用（仅验证结构，不调用 LLM）
        // PlansAnalyzer 依赖 provider，无法在单元测试中实例化
        // 此测试验证 PlansResponse 解析逻辑
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "deterministic",
                    "step_goal": "读取配置文件",
                    "expected_output": "配置对象",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": [],
                    "actions": [
                        {"order": 1, "tool_name": "mcp__fs__read_file", "parameters": {"path": "config.json"}}
                    ]
                },
                {
                    "order": 2,
                    "step_type": "exploratory",
                    "step_goal": "解析配置内容",
                    "expected_output": "解析后的配置对象",
                    "depends_on": [1],
                    "input": ["{{step_1.output}}"],
                    "success_criteria": ["配置解析成功"],
                    "actions": []
                }
            ]
        }"#;

        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(resp.steps.len(), 2);
        assert_eq!(resp.steps[0].order, 1);
        assert_eq!(resp.steps[1].order, 2);
        assert_eq!(resp.steps[1].input, vec!["{{step_1.output}}".to_string()]);
    }

    #[test]
    fn test_plans_analyzer_empty_steps_error() {
        // 空 steps 数组应返回错误
        let json = r#"{"steps": []}"#;
        let err = parse_plans_response(json).expect_err("should fail");
        assert!(err.to_string().contains("steps array is empty"));
    }

    #[test]
    fn test_plans_analyzer_reasoning_step() {
        // reasoning 类型的步骤（actions 应为空）
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "reasoning",
                    "step_goal": "对比两产品参数并给出建议",
                    "expected_output": "详细对比报告",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": ["给出明确推荐"],
                    "actions": []
                }
            ]
        }"#;

        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(resp.steps.len(), 1);
        // reasoning 类型暂未导出到顶层，通过 step_type 字段验证
        let json_str = serde_json::to_string(&resp.steps[0]).unwrap();
        assert!(json_str.contains("\"reasoning\""));
    }
}