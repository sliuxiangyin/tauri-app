//! 确定性步骤执行器
//!
//! 负责执行步骤类型为 `Deterministic` 的计划步骤，调用工具执行器完成实际工具调用。

use std::sync::Arc;

use serde_json::{Map, Value};

use super::{PlanError, StepContext};
use crate::provider::llm::llm_tool_trait::ToolExecutor;
use crate::provider::llm::types::{FunctionCall, PlanStep, SubAction};

/// 执行确定性步骤
///
/// - 从 actions[0] 获取 tool_name 和 parameters
/// - 调用 tool_executor 执行工具
/// - 将执行结果更新到 actions[0].output
pub(crate) async fn execute_step(
    tool_executor: &Arc<dyn ToolExecutor>,
    step: &PlanStep,
    context: &StepContext,
) -> Result<PlanStep, PlanError> {
    // 1. 从 actions 列表获取第一个 SubAction（确定性步骤的初始动作）
    let first_action = step.actions.first().ok_or_else(|| {
        PlanError::ToolError(crate::provider::llm::llm_tool_trait::ToolExecError {
            name: "deterministic".to_string(),
            message: "Deterministic step must have at least one action".to_string(),
        })
    })?;

    let tool_name = first_action.tool_name.clone();
    let mut arguments = if first_action.parameters.is_object() {
        first_action.parameters.as_object().unwrap().clone()
    } else {
        Map::new()
    };

    // 2. 如果有依赖，把所有前置步骤的输出合并到 arguments 中
    //    每个依赖的输出以 "__prev_step_<order>_output" 为键注入，
    //    这样确定性步骤可以引用多个前置步骤的结果
    for &dep_order in &step.depends_on {
        if let Some(dep_output) = context.get_output(dep_order) {
            arguments.insert(
                format!("__prev_step_{}_output", dep_order),
                Value::String(dep_output.to_string()),
            );
        }
    }

    let call = FunctionCall {
        id: format!("plan_step_{}", step.order),
        name: tool_name.clone(),
        arguments: Value::Object(arguments.clone()),
    };

    // 3. 执行工具调用
    let result = tool_executor
        .execute_tool(call)
        .await
        .map_err(PlanError::from)?;

    // 4. 转换为字符串
    let output = if result.is_string() {
        result.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
    };

    // 5. 更新 SubAction 的 output
    let mut updated_actions = step.actions.clone();
    if let Some(first) = updated_actions.first_mut() {
        first.output = Some(output.clone());
    }

    // 6. 返回更新后的 PlanStep
    Ok(PlanStep {
        order: step.order,
        step_type: step.step_type,
        step_goal: step.step_goal.clone(),
        expected_output: step.expected_output.clone(),
        depends_on: step.depends_on.clone(),
        input: step.input.clone(),
        success_criteria: step.success_criteria.clone(),
        actions: updated_actions,
    })
}