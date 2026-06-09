//! 探索性步骤执行器
//!
//! 负责执行步骤类型为 `Exploratory` 的计划步骤：
//! 1. 构建上下文信息（步骤目标、前置输出、历史结果、可用工具）
//! 2. 调用 LLM 决定使用哪个工具及参数
//! 3. 将探索性步骤转换为确定性步骤后执行

use std::sync::Arc;

use serde_json::Value;

use super::{PlanError, StepContext};
use crate::provider::llm::agent::IntentAnalyzer;
use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::providers::LlmProvider;
use crate::provider::llm::types::{ChatMessage, PlanStep, Role};
use crate::provider::llm::types::StepType;
use crate::provider::llm::agent::plan_executor::step_executor::execute_step;

/// 执行探索性步骤
///
/// 探索性步骤需要在执行时根据上下文决定工具和参数。
/// 如果没有配置 LLM Provider，返回错误。
pub(crate) async fn execute_exploratory_step(
    llm_provider: &Option<Arc<dyn LlmProvider>>,
    tool_executor: Arc<dyn ToolExecutor>,
    available_tools: &[String],
    step: &PlanStep,
    context: &StepContext,
) -> Result<String, PlanError> {
    let Some(llm) = llm_provider else {
        return Err(PlanError::ToolError(ToolExecError {
            name: "exploratory".to_string(),
            message: "Exploratory step requires LLM provider but none configured".to_string(),
        }));
    };

    // 构建上下文信息
    let context_info = build_exploratory_context(available_tools, step, context);

    // 调用 LLM 决定工具和参数
    let (tool_name, parameters) = decide_tool_for_exploratory(llm, &context_info).await?;

    // 使用 LLM 决定的信息执行步骤（转换为确定性步骤）
    let step_with_decision = PlanStep {
        order: step.order,
        step_type: StepType::Deterministic,
        tool_name,
        parameters,
        step_goal: step.step_goal.clone(),
        expected_output: step.expected_output.clone(),
        depends_on: step.depends_on,
    };

    execute_step(&tool_executor, &step_with_decision, context).await
}

/// 构建探索性步骤的上下文信息
///
/// 包含：步骤目标、前置步骤输出、已执行步骤历史、可用工具列表。
fn build_exploratory_context(
    available_tools: &[String],
    step: &PlanStep,
    context: &StepContext,
) -> String {
    let mut info = String::new();

    // 添加步骤目标
    info.push_str(&format!("【步骤目标】\n{}\n\n", step.step_goal));

    // 添加前置步骤输出（如果有依赖）
    if let Some(dep_order) = step.depends_on {
        if let Some(dep_output) = context.get_output(dep_order) {
            info.push_str(&format!("【前置步骤输出】\n{}\n\n", dep_output));
        }
    }

    // 添加历史结果
    if !context.results.is_empty() {
        let history = context
            .results
            .iter()
            .map(|r| {
                format!(
                    "步骤{}: {} - {}",
                    r.order,
                    r.tool_name,
                    if r.success { "成功" } else { "失败" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        info.push_str(&format!("【已执行步骤】\n{}\n\n", history));
    }

    // 添加可用工具列表
    if !available_tools.is_empty() {
        info.push_str(&format!("【可用工具】\n{}\n\n", available_tools.join(", ")));
    }

    info
}

/// 调用 LLM 决定探索性步骤的工具和参数
async fn decide_tool_for_exploratory(
    llm: &Arc<dyn LlmProvider>,
    context_info: &str,
) -> Result<(String, Value), PlanError> {
    let user_message = format!(
        r#"【探索性步骤：需要决定工具和参数】
{}

请根据上述信息，从可用工具中选择最合适的工具并决定参数。

要求：
1. 只返回 JSON，不要其他文字
2. 格式：{{"tool_name": "工具名", "parameters": {{}}}}
3. 只选择一个工具
"#,
        context_info
    );

    let messages = vec![
        ChatMessage::new(
            Role::System,
            "你是一个智能助手，负责在探索性步骤中决定使用哪个工具以及参数。请只返回 JSON 格式的回答。",
        ),
        ChatMessage::new(Role::User, &user_message),
    ];

    let analyzer = IntentAnalyzer::new(llm.clone());
    let response = analyzer
        .decision_raw(messages, vec![])
        .await
        .map_err(|e| {
            PlanError::ToolError(ToolExecError {
                name: "exploratory".to_string(),
                message: format!("Failed to decide tool: {}", e),
            })
        })?;

    // 解析 LLM 响应（提取 JSON 部分）
    let json_str = if response.contains('{') {
        let start = response.find('{').unwrap();
        let end = response.rfind('}').unwrap_or(response.len() - 1);
        &response[start..=end]
    } else {
        &response
    };

    let json: Value =
        serde_json::from_str(json_str).map_err(|e| PlanError::ToolError(ToolExecError {
            name: "exploratory".to_string(),
            message: format!("Failed to parse LLM response: {}", e),
        }))?;

    let tool_name = json
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let parameters = json.get("parameters").cloned().unwrap_or(serde_json::json!({}));

    if tool_name.is_empty() {
        return Err(PlanError::ToolError(ToolExecError {
            name: "exploratory".to_string(),
            message: "LLM did not return a tool name".to_string(),
        }));
    }

    Ok((tool_name, parameters))
}
