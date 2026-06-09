//! 确定性步骤执行器
//!
//! 负责执行步骤类型为 `Deterministic` 的计划步骤，调用工具执行器完成实际工具调用。

use std::sync::Arc;

use serde_json::Value;

use super::{PlanError, StepContext};
use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::types::{FunctionCall, PlanStep};

/// 执行确定性步骤
///
/// - 构建 FunctionCall（合并 step.parameters 与前置步骤输出）
/// - 调用 tool_executor 执行工具
/// - 将执行结果转为字符串返回
pub(crate) async fn execute_step(
    tool_executor: &Arc<dyn ToolExecutor>,
    step: &PlanStep,
    context: &StepContext,
) -> Result<String, PlanError> {
    // 1. 首先使用 step.parameters（LLM 生成的参数）
    let mut arguments = if step.parameters.is_object() {
        step.parameters.as_object().unwrap().clone()
    } else {
        serde_json::Map::new()
    };

    // 2. 如果有依赖，添加前置步骤的输出（作为 __prev_step_output）
    if let Some(dep_order) = step.depends_on {
        if let Some(dep_output) = context.get_output(dep_order) {
            arguments.insert(
                "__prev_step_output".to_string(),
                Value::String(dep_output.to_string()),
            );
        }
    }

    let call = FunctionCall {
        id: format!("plan_step_{}", step.order),
        name: step.tool_name.clone(),
        arguments: Value::Object(arguments),
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

    Ok(output)
}
