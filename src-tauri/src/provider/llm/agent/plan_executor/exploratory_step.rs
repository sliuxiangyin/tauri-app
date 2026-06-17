//! 探索性步骤执行器
//!
//! 探索性步骤是一个完整的 Agent 循环：
//! 1. LLM 决策下一步工具
//! 2. 执行工具
//! 3. LLM 判断是否达成目标
//! 4. 未达成则继续循环，直到达成或达到最大调用次数
//!
//! 提示词统一从 `prompts::exploratory_prompt` 模块获取

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use serde_json::Value;

use super::message_context::MessageContext;
use super::types::StepExecResult;
use super::{PlanError, StepContext};
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::prompts::{
    build_goal_check_message, goal_check_system_prompt, parse_goal_check_response,
};
use crate::provider::llm::providers::LlmProvider;
use crate::provider::llm::types::{ChatMessage, ChatRequest, FunctionCall, PlanStep, Role, SubAction, ToolDefinition};

/// LLM 决策结果枚举
///
/// 表示 LLM 在 Agent 循环中的决策类型：
/// - `ToolCall`: LLM 认为需要继续执行工具
/// - `StepComplete`: LLM 认为步骤目标已达成
#[derive(Debug)]
pub(crate) enum LlmDecision {
    /// 执行工具调用（可能多个）
    ToolCall(Vec<FunctionCall>),
    /// 步骤完成信号，携带 LLM 的响应文本
    StepComplete(String),
}

/// 执行探索性步骤（Agent 循环模式）
///
/// 探索性步骤是一个完整的 Agent 循环：
/// 1. LLM 决策下一步工具
/// 2. 执行工具
/// 3. LLM 判断是否达成目标
/// 4. 未达成则继续循环
///
/// # 参数
/// - `llm_provider`: LLM Provider（必需）
/// - `tool_executor`: 工具执行器
/// - `available_tools`: 可用工具完整 schema
/// - `context`: 步骤执行上下文（已执行步骤的输出，用于 `{{step_N}}` 引用）
/// - `msg_ctx`: **Plan 级别** LLM 消息上下文（跨步骤累积，构造于 `execute_plan`）
/// - `step`: 探索性步骤
/// - `abort_flag`: 中止标志
/// - `model`: LLM 模型名称
/// - `max_calls`: 最大工具调用次数
///
/// # 返回
/// 返回更新后的 `PlanStep`
pub(crate) async fn execute_exploratory_step(
    llm_provider: &Option<Arc<dyn LlmProvider>>,
    tool_executor: Arc<dyn ToolExecutor>,
    available_tools: &[ToolDefinition],
    context: &StepContext,
    msg_ctx: &mut MessageContext,
    step: &PlanStep,
    abort_flag: Arc<AtomicBool>,
    model: &str,
    max_calls: u8,
) -> Result<StepExecResult, PlanError> {
    let start_time = Instant::now();

    let Some(llm) = llm_provider else {
        return Err(PlanError::ToolError(ToolExecError {
            name: "exploratory".to_string(),
            message: "Exploratory step requires LLM provider but none configured".to_string(),
        }));
    };

    tracing::info!(
        "[Exploratory] Starting Agent loop for step: {}, max_calls={}",
        step.step_goal,
        max_calls
    );

    // 推入本步骤目标到 Plan 级别消息上下文
    // 后续 LLM 决策时会从 msg_ctx.messages() 中看到本步骤目标 + 历史累积
    msg_ctx.push_step_goal(step.order, &step.step_goal, step.expected_output.as_deref());

    // 工具调用历史（用于判断目标达成）：(tool_name, parameters, output)
    let mut tool_calls_history: Vec<(String, Value, String)> = Vec::new();

    // 收集所有工具执行结果作为最终输出
    let mut all_outputs: Vec<String> = Vec::new();

    // Agent 循环
    for call_count in 0..max_calls {
        // 检查中止标志
        if abort_flag.load(Ordering::SeqCst) {
            return Err(PlanError::Aborted);
        }

        tracing::debug!(
            "[Exploratory] Call {}/{} for step: {}",
            call_count + 1,
            max_calls,
            step.step_goal
        );

        // 1. LLM 决策：工具调用 OR 步骤完成信号
        let decision = llm_decide_next_tool(
            llm,
            msg_ctx.messages(),
            available_tools,
            model,
            abort_flag.clone(),
        )
        .await?;

        match decision {
            LlmDecision::ToolCall(tool_calls) => {
                // 2a. 依次执行所有工具调用
                for tool_call in tool_calls {
                    if abort_flag.load(Ordering::SeqCst) {
                        return Err(PlanError::Aborted);
                    }

                    let (tool_name, params, result) = llm_execute_tool(
                        tool_executor.clone(),
                        tool_call,
                    )
                    .await?;

                    tracing::info!(
                        "[Exploratory] Tool executed: {} -> {}",
                        tool_name,
                        result.chars().take(50).collect::<String>()
                    );

                    // 记录工具调用并添加到消息历史
                    tool_calls_history.push((tool_name.clone(), params, result.clone()));
                    all_outputs.push(format!("[{}] {}", tool_name, result));
                    msg_ctx.push_tool_result(step.order, &tool_name, &result);
                }
                // 继续循环，让 LLM 决定下一步
            }

            LlmDecision::StepComplete(llm_reason) => {
                // 2b. LLM 认为步骤完成，进行验证
                tracing::info!(
                    "[Exploratory] LLM signaled STEP_COMPLETE, verifying: {}",
                    llm_reason.chars().take(100).collect::<String>()
                );

                // 构建 (name, output) 视图传给 goal check
                let goal_check_history: Vec<(String, String)> = tool_calls_history
                    .iter()
                    .map(|(name, _, output)| (name.clone(), output.clone()))
                    .collect();

                let (achieved, reason) = llm_check_goal(
                    llm,
                    &step.step_goal,
                    &goal_check_history,
                    model,
                    abort_flag.clone(),
                )
                .await?;

                tracing::debug!(
                    "[Exploratory] Goal check after STEP_COMPLETE: achieved={}, reason={}",
                    achieved,
                    reason
                );

                if achieved {
                    // 验证通过，步骤成功完成
                    tracing::info!(
                        "[Exploratory] Step completed successfully after {} calls",
                        call_count + 1
                    );

                    // 构建 SubAction 列表
                    let mut actions = Vec::new();
                    for (idx, (name, params, result)) in tool_calls_history.iter().enumerate() {
                        actions.push(SubAction {
                            order: (idx + 1) as u8,
                            tool_name: name.clone(),
                            parameters: params.clone(),
                            output: Some(result.clone()),
                        });
                    }

                    return Ok(StepExecResult {
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
                        duration_ms: start_time.elapsed().as_millis() as u64,
                    });
                } else {
                    // 验证失败，告知 LLM 原因，继续执行
                    tracing::warn!(
                        "[Exploratory] STEP_COMPLETE verification failed: {}",
                        reason
                    );
                    msg_ctx.push_goal_check(step.order, false, &reason);
                    // 继续循环，LLM 会收到失败反馈并继续执行
                }
            }
        }
    }

    // 达到最大调用次数，目标未达成
    tracing::warn!(
        "[Exploratory] Step failed: reached max_calls={} without achieving goal",
        max_calls
    );

    Err(PlanError::ToolError(ToolExecError {
        name: "exploratory".to_string(),
        message: format!(
            "Step '{}' failed after {} calls: {}",
            step.step_goal,
            max_calls,
            tool_calls_history
                .last()
                .map(|(_, _, r)| r.as_str())
                .unwrap_or("unknown")
        ),
    }))
}

/// LLM 决策下一步工具
///
/// 返回 LLM 的决策结果：
/// - `LlmDecision::ToolCall`: 需要执行工具调用
/// - `LlmDecision::StepComplete`: LLM 认为步骤目标已达成
async fn llm_decide_next_tool(
    llm: &Arc<dyn LlmProvider>,
    messages: &[ChatMessage],
    available_tools: &[ToolDefinition],
    model: &str,
    abort_flag: Arc<AtomicBool>,
) -> Result<LlmDecision, PlanError> {
    let req = ChatRequest {
        messages: messages.to_vec(),
        model: model.to_string(),
        temperature: 0.7,
        max_tokens: None,
        tools: Some(available_tools.to_vec()),
    };

    let stream = llm
        .stream_chat(req, abort_flag)
        .await
        .map_err(|e| PlanError::ToolError(ToolExecError {
            name: "exploratory".to_string(),
            message: format!("LLM stream error: {}", e),
        }))?;

    // 使用 process_tool_batch 获取工具调用
    let result = crate::provider::llm::ordinary::process_tool_batch(
        stream,
        None,  // 这里不执行工具，只获取 LLM 的决策
        None,
    )
    .await
    .map_err(|e| PlanError::ToolError(ToolExecError {
        name: "exploratory".to_string(),
        message: format!("process_tool_batch error: {}", e),
    }))?;

    // 检查 LLM 是否输出了 STEP_COMPLETE 信号
    if result.text.contains("STEP_COMPLETE") {
        tracing::info!(
            "[Exploratory] LLM signaled STEP_COMPLETE: {}",
            result.text.chars().take(100).collect::<String>()
        );
        return Ok(LlmDecision::StepComplete(result.text));
    }

    // 打印所有 LLM 返回的工具调用，用于调试
    tracing::info!(
        "[Exploratory] LLM returned {} tool call(s): {:?}",
        result.tool_calls.len(),
        result.tool_calls.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // 解析所有工具调用
    if result.tool_calls.is_empty() {
        Err(PlanError::ToolError(ToolExecError {
            name: "exploratory".to_string(),
            message: format!("No tool call and no STEP_COMPLETE from LLM: {}", result.text),
        }))
    } else {
        let calls = result
            .tool_calls
            .into_iter()
            .map(|call| FunctionCall {
                id: format!(
                    "exploratory_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                ),
                name: call.name,
                arguments: call.arguments,
            })
            .collect();
        Ok(LlmDecision::ToolCall(calls))
    }
}

/// LLM 执行工具
///
/// 返回 (tool_name, parameters, output) 三元组，保留 LLM 决策的参数
async fn llm_execute_tool(
    tool_executor: Arc<dyn ToolExecutor>,
    call: FunctionCall,
) -> Result<(String, Value, String), PlanError> {
    let params = call.arguments.clone();
    let result = tool_executor
        .execute_tool(call.clone())
        .await
        .map_err(|e| PlanError::ToolError(e))?;

    let output = if result.is_string() {
        result.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
    };

    Ok((call.name, params, output))
}

/// LLM 判断目标是否达成
async fn llm_check_goal(
    llm: &Arc<dyn LlmProvider>,
    step_goal: &str,
    tool_calls_history: &[(String, String)],
    model: &str,
    abort_flag: Arc<AtomicBool>,
) -> Result<(bool, String), PlanError> {
    let user_message = build_goal_check_message(step_goal, tool_calls_history);

    let req = ChatRequest {
        messages: vec![
            ChatMessage::new(Role::System, goal_check_system_prompt()),
            ChatMessage::new(Role::User, &user_message),
        ],
        model: model.to_string(),
        temperature: 0.3,
        max_tokens: None,
        tools: None,
    };

    let stream = llm
        .stream_chat(req, abort_flag)
        .await
        .map_err(|e| PlanError::ToolError(ToolExecError {
            name: "exploratory".to_string(),
            message: format!("LLM goal check error: {}", e),
        }))?;

    // 流式收集完整响应
    let mut response = String::new();
    let mut stream = stream;
    while let Some(item) = stream.next().await {
        match item {
            Ok(LlmStreamEvent::TextDelta { text }) => {
                response.push_str(&text);
            }
            Ok(_) => {} // 忽略其他事件
            Err(e) => return Err(PlanError::ToolError(ToolExecError {
                name: "exploratory".to_string(),
                message: format!("Stream error: {}", e),
            })),
        }
    }

    Ok(parse_goal_check_response(&response))
}
