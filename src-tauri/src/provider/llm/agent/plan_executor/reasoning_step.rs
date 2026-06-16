//! 推理性步骤执行器
//!
//! 负责执行 `Reasoning` 类型的计划步骤：
//! - 收集依赖步骤的输出作为上下文
//! - 调用 LLM 生成推理结果（不使用 Function Calling）
//! - 将 LLM 输出写入 SubAction

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;

use super::types::StepExecResult;
use super::{PlanError, StepContext};
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::llm_tool_trait::ToolExecError;
use crate::provider::llm::prompts::{
    build_reasoning_message, reasoning_system_prompt,
};
use crate::provider::llm::providers::LlmProvider;
use crate::provider::llm::types::{ChatMessage, ChatRequest, PlanStep, Role, SubAction};

/// 执行推理性步骤
///
/// Reasoning 步骤不需要工具调用，而是调用 LLM 进行推理生成文本内容：
/// - Summarization（摘要）
/// - Classification（分类）
/// - Comparison（对比）
/// - Information extraction（信息提取）
/// - Report generation（报告生成）
/// - Decision making（决策）
///
/// # 参数
/// - `llm_provider`: LLM Provider（必需）
/// - `step`: 推理性步骤
/// - `context`: 步骤执行上下文（依赖步骤的输出）
/// - `model`: LLM 模型名称
/// - `abort_flag`: 中止标志
///
/// # 返回
/// 返回更新后的 `PlanStep`，LLM 输出写入 actions[0].output
pub(crate) async fn execute_reasoning_step(
    llm_provider: &Option<Arc<dyn LlmProvider>>,
    step: &PlanStep,
    context: &StepContext,
    model: &str,
    abort_flag: Arc<AtomicBool>,
) -> Result<StepExecResult, PlanError> {
    let start_time = Instant::now();

    let Some(llm) = llm_provider else {
        return Err(PlanError::ToolError(ToolExecError {
            name: "reasoning".to_string(),
            message: "Reasoning step requires LLM provider but none configured".to_string(),
        }));
    };

    tracing::info!(
        "[Reasoning] Starting reasoning step: {}",
        step.step_goal
    );

    // 1. 收集依赖步骤的输出作为上下文
    let context_inputs = collect_dependency_outputs(step, context);

    // 2. 构建消息
    let user_message = build_reasoning_message(&step.step_goal, &context_inputs);
    let messages = vec![
        ChatMessage::new(Role::System, reasoning_system_prompt()),
        ChatMessage::new(Role::User, &user_message),
    ];

    // 3. 调用 LLM（不使用 Function Calling）
    let req = ChatRequest {
        messages,
        model: model.to_string(),
        temperature: 0.3,
        max_tokens: None,
        tools: None,
    };

    let stream = llm
        .stream_chat(req, abort_flag)
        .await
        .map_err(|e| PlanError::ToolError(ToolExecError {
            name: "reasoning".to_string(),
            message: format!("LLM stream error: {}", e),
        }))?;

    // 4. 流式收集完整响应
    let mut response = String::new();
    let mut stream = stream;
    while let Some(item) = stream.next().await {
        match item {
            Ok(LlmStreamEvent::TextDelta { text }) => {
                response.push_str(&text);
            }
            Ok(_) => {} // 忽略其他事件
            Err(e) => {
                return Err(PlanError::ToolError(ToolExecError {
                    name: "reasoning".to_string(),
                    message: format!("Stream error: {}", e),
                }));
            }
        }
    }

    tracing::info!(
        "[Reasoning] Step completed, output length={}",
        response.len()
    );

    // 5. 构建 SubAction（reasoning 步骤记录一个虚拟 action）
    let actions = vec![SubAction {
        order: 1,
        tool_name: "reasoning".to_string(),
        parameters: serde_json::json!({ "goal": step.step_goal }),
        output: Some(response),
    }];

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // 6. 返回 StepExecResult
    Ok(StepExecResult {
        step: PlanStep {
            order: step.order,
            step_type: step.step_type,
            step_goal: step.step_goal.clone(),
            expected_output: step.expected_output.clone(),
            depends_on: step.depends_on.clone(),
            input: step.input.clone(),
            success_criteria: step.success_criteria.clone(),
            actions,
        },
        duration_ms,
    })
}

/// 收集依赖步骤的输出，拼接为上下文字符串
fn collect_dependency_outputs(step: &PlanStep, context: &StepContext) -> String {
    if step.depends_on.is_empty() {
        return String::new();
    }

    let parts: Vec<String> = step
        .depends_on
        .iter()
        .filter_map(|&dep_order| {
            context.get_output(dep_order).map(|output| {
                format!("步骤{} 输出:\n{}", dep_order, output)
            })
        })
        .collect();

    if parts.is_empty() {
        String::new()
    } else {
        parts.join("\n\n")
    }
}
