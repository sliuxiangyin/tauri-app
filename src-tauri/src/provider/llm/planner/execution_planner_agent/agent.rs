//! Execution Planner Agent
//!
//! 使用 LLM 在领域规则约束下将 Task Stage 分解为 Execution Step 序列。
//!
//! ## 使用方式
//!
//! ```ignore
//! let agent = ExecutionPlannerAgent::new(provider.clone())
//!     .with_model("gpt-4o".into())
//!     .with_stage_goal("在搜索页输入关键词并执行搜索".into())
//!     .with_stage_domain("browser".into())
//!     .with_stage_inputs(inputs_json)
//!     .with_stage_outputs(outputs_json)
//!     .with_planning_rules(rules)
//!     .with_available_tools(tools_description)
//!     .with_runtime_context("当前页面: https://www.baidu.com".into())
//!     .with_previous_stage_outputs(previous_outputs_json);
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
use super::prompt::EXECUTION_PLANNER_PROMPT;
use super::types::ExecutionPlan;

/// Execution Planner Agent
#[derive(Clone)]
pub struct ExecutionPlannerAgent {
    /// 共享的 LLM Agent 基类
    base: LlmAgentBase,
    /// 当前 Task Stage 的业务目标（注入到 prompt {{STAGE_GOAL}}）
    stage_goal: String,
    /// 当前 Stage 所属领域（注入到 prompt {{STAGE_DOMAIN}}）
    stage_domain: String,
    /// 当前 Stage 的输入参数 JSON（注入到 prompt {{STAGE_INPUTS}}）
    stage_inputs: String,
    /// 当前 Stage 期望的产出 JSON（注入到 prompt {{STAGE_OUTPUTS}}）
    stage_outputs: String,
    /// 当前领域的 Planning Rules（注入到 prompt {{PLANNING_RULES}}）
    planning_rules: String,
    /// 当前领域可用工具列表描述（注入到 prompt {{AVAILABLE_TOOLS}}）
    available_tools: String,
    /// 运行环境信息（注入到 prompt {{RUNTIME_CONTEXT}}）
    runtime_context: String,
    /// 前序 Stage 的实际输出值（注入到 prompt {{PREVIOUS_STAGE_OUTPUTS}}）
    previous_stage_outputs: String,
}

impl ExecutionPlannerAgent {
    /// 创建 ExecutionPlannerAgent（使用 provider 默认模型）
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            base: LlmAgentBase::new(provider),
            stage_goal: String::new(),
            stage_domain: String::new(),
            stage_inputs: String::new(),
            stage_outputs: String::new(),
            planning_rules: String::new(),
            available_tools: String::new(),
            runtime_context: String::new(),
            previous_stage_outputs: String::new(),
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

    /// 设置 Stage 目标
    ///
    /// 将注入到 prompt 的 `{{STAGE_GOAL}}`。
    pub fn with_stage_goal(self, goal: String) -> Self {
        Self {
            base: self.base,
            stage_goal: goal,
            ..self
        }
    }

    /// 设置 Stage 领域
    ///
    /// 将注入到 prompt 的 `{{STAGE_DOMAIN}}`。
    pub fn with_stage_domain(self, domain: String) -> Self {
        Self {
            base: self.base,
            stage_domain: domain,
            ..self
        }
    }

    /// 设置 Stage 输入参数（JSON 格式）
    ///
    /// 将注入到 prompt 的 `{{STAGE_INPUTS}}`。
    /// 建议通过 `serde_json::to_string_pretty(&stage.inputs)` 生成。
    pub fn with_stage_inputs(self, inputs: String) -> Self {
        Self {
            base: self.base,
            stage_inputs: inputs,
            ..self
        }
    }

    /// 设置 Stage 期望产出（JSON 格式）
    ///
    /// 将注入到 prompt 的 `{{STAGE_OUTPUTS}}`。
    /// 建议通过 `serde_json::to_string_pretty(&stage.outputs)` 生成。
    pub fn with_stage_outputs(self, outputs: String) -> Self {
        Self {
            base: self.base,
            stage_outputs: outputs,
            ..self
        }
    }

    /// 设置领域规划规则
    ///
    /// 将注入到 prompt 的 `{{PLANNING_RULES}}`。可为空字符串。
    pub fn with_planning_rules(self, rules: String) -> Self {
        Self {
            base: self.base,
            planning_rules: rules,
            ..self
        }
    }

    /// 根据 domain 从指定目录自动加载 Planning Rules
    ///
    /// 查找 `{rules_dir}/{domain}_rules.yaml` 文件，读取内容并注入到 prompt。
    /// 若文件不存在，planning_rules 设为空字符串（不影响执行）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// let agent = ExecutionPlannerAgent::new(provider)
    ///     .with_stage_domain("browser".into())
    ///     .load_planning_rules("src-tauri/src/provider/llm/planner/planning_rules");
    /// // 自动加载 browser_rules.yaml
    /// ```
    pub fn load_planning_rules(self, rules_dir: &str) -> Self {
        let file_path = format!("{}/{}_rules.yaml", rules_dir, self.stage_domain);
        let rules = match std::fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(_) => {
                tracing::warn!(
                    domain = %self.stage_domain,
                    path = %file_path,
                    "Planning rules file not found, proceeding without rules"
                );
                String::new()
            }
        };
        Self {
            base: self.base,
            planning_rules: rules,
            ..self
        }
    }

    /// 设置可用工具列表描述
    ///
    /// 将注入到 prompt 的 `{{AVAILABLE_TOOLS}}`。
    /// 格式示例：`- browser.click (browser_interaction): 点击页面元素`
    pub fn with_available_tools(self, tools: String) -> Self {
        Self {
            base: self.base,
            available_tools: tools,
            ..self
        }
    }

    /// 设置运行环境信息
    ///
    /// 将注入到 prompt 的 `{{RUNTIME_CONTEXT}}`。
    pub fn with_runtime_context(self, context: String) -> Self {
        Self {
            base: self.base,
            runtime_context: context,
            ..self
        }
    }

    /// 设置前序 Stage 的实际输出值
    ///
    /// 将注入到 prompt 的 `{{PREVIOUS_STAGE_OUTPUTS}}`。
    pub fn with_previous_stage_outputs(self, outputs: String) -> Self {
        Self {
            base: self.base,
            previous_stage_outputs: outputs,
            ..self
        }
    }

    /// 非流式执行：调用 LLM 并返回 ExecutionPlan
    pub async fn run(&self) -> Result<ExecutionPlan, LlmError> {
        let messages = self.build_messages();
        run_llm(&self.base, messages, |r| self.parse_response(r)).await
    }

    /// 流式执行：返回 text_stream + parse_future
    pub async fn run_streaming(&self) -> Result<StreamingResponse<ExecutionPlan>, LlmError> {
        let messages = self.build_messages();
        let parse_fn = move |response: &str| -> Result<ExecutionPlan, LlmError> {
            let json_str = Self::extract_json(response);
            serde_json::from_str(json_str).map_err(|e| {
                LlmError::ParseError(format!(
                    "Failed to parse ExecutionPlan JSON: {}\n--- response ---\n{}",
                    e, response
                ))
            })
        };
        run_streaming_llm(&self.base, messages, parse_fn).await
    }
}

impl LlmAgent for ExecutionPlannerAgent {
    type Output = ExecutionPlan;

    fn base(&self) -> &LlmAgentBase {
        &self.base
    }

    fn build_messages(&self) -> Vec<ChatMessage> {
        let system_prompt = self.build_system_prompt();
        // user message 使用 stage goal，让 LLM 聚焦当前阶段
        let user_message = format!("【Stage 目标】\n{}", self.stage_goal);

        vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, user_message),
        ]
    }

    fn parse_response(&self, response: &str) -> Result<ExecutionPlan, LlmError> {
        let json_str = Self::extract_json(response);
        serde_json::from_str(json_str).map_err(|e| {
            LlmError::ParseError(format!(
                "Failed to parse ExecutionPlan JSON: {}\n--- response ---\n{}",
                e, response
            ))
        })
    }
}

impl ExecutionPlannerAgent {
    /// 替换 prompt 中的 8 个模板变量
    fn build_system_prompt(&self) -> String {
        EXECUTION_PLANNER_PROMPT
            .replace("{{STAGE_GOAL}}", &self.stage_goal)
            .replace("{{STAGE_DOMAIN}}", &self.stage_domain)
            .replace("{{STAGE_INPUTS}}", &self.stage_inputs)
            .replace("{{STAGE_OUTPUTS}}", &self.stage_outputs)
            .replace("{{PLANNING_RULES}}", &self.planning_rules)
            .replace("{{AVAILABLE_TOOLS}}", &self.available_tools)
            .replace("{{RUNTIME_CONTEXT}}", &self.runtime_context)
            .replace("{{PREVIOUS_STAGE_OUTPUTS}}", &self.previous_stage_outputs)
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
