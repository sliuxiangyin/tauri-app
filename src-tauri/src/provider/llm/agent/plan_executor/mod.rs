//! 计划执行器
//!
//! 按步骤顺序执行 IntentPlan，支持依赖检查和结果验证
//!
//! 采用混合模式：正常情况下按计划顺序执行；步骤失败时调用 LLM 分析原因并决定后续动作
//!
//! 执行流程遵循 Goal → Plan → Execute → Observe → Replan → Complete 循环
//!
//! ## 模块结构
//! - `deterministic_step.rs`：确定性步骤执行（调用工具）
//! - `reasoning_step.rs`：推理性步骤执行（LLM 推理生成内容）
//! - `exploratory_step.rs`：探索性步骤执行（LLM 动态选工具）

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use tracing::{debug, error, info, warn};

use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::providers::LlmProvider;
use crate::provider::llm::agent::types::StepType;
use crate::provider::llm::prompts::{
    build_replan_message, parse_replan_response, replan_system_prompt,
};
use crate::provider::llm::types::{ChatMessage, ChatRequest, PlanStep, Role, ToolDefinition};

pub(crate) mod deterministic_step;
pub(crate) mod exploratory_step;
pub(crate) mod message_context;
pub(crate) mod reasoning_step;
pub(crate) mod types;

use deterministic_step::execute_deterministic_step;
use exploratory_step::execute_exploratory_step;
use message_context::MessageContext;
use reasoning_step::execute_reasoning_step;

// 重新导出所有数据类型，保持外部 `crate::provider::llm::agent::plan_executor::XXX` 路径不变
// 注：必须用 `pub use`（非 `pub(crate) use`），否则上层 `pub use plan_executor::{PlanResult, ...}`
// 会因 E0365 失败（crate 私有类型不能被 pub re-export）
pub use types::*;

use crate::provider::llm::prompts::exploratory_prompt::re_act_system_prompt;

/// 步骤执行记录（存储已执行的步骤及耗时，供后续步骤引用）
struct StepRecord {
    step: PlanStep,
    duration_ms: u64,
}

/// 步骤执行上下文（存储已执行的步骤，供后续步骤引用）
pub(crate) struct StepContext {
    /// 已执行的步骤记录列表
    records: Vec<StepRecord>,
    /// 被跳过的步骤 order 列表
    skipped_steps: Vec<u8>,
}

impl StepContext {
    fn new() -> Self {
        Self {
            records: Vec::new(),
            skipped_steps: Vec::new(),
        }
    }

    /// 获取指定步骤的输出
    fn get_output(&self, order: u8) -> Option<String> {
        self.records
            .iter()
            .find(|r| r.step.order == order)
            .and_then(|r| r.step.actions.last())  // 取最后一个 SubAction 的输出
            .and_then(|a| a.output.as_ref())
            .map(|s| s.clone())
    }

    /// 检查指定步骤是否已被跳过
    fn is_skipped(&self, order: u8) -> bool {
        self.skipped_steps.contains(&order)
    }

    /// 获取已执行的步骤列表
    fn get_executed_steps(&self) -> Vec<&PlanStep> {
        self.records.iter().map(|r| &r.step).collect()
    }

    /// 获取指定步骤的耗时
    fn get_duration_ms(&self, order: u8) -> u64 {
        self.records
            .iter()
            .find(|r| r.step.order == order)
            .map(|r| r.duration_ms)
            .unwrap_or(0)
    }

    /// 获取历史摘要（用于 LLM 上下文）
    fn get_history_summary(&self) -> String {
        if self.records.is_empty() {
            return String::new();
        }

        self.records
            .iter()
            .filter_map(|r| {
                let output = r.step
                    .actions
                    .last()
                    .and_then(|a| a.output.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or("（无输出）");

                let tool_name = r.step
                    .actions
                    .first()
                    .map(|a| a.tool_name.as_str())
                    .unwrap_or("（无工具）");

                Some(format!(
                    "步骤{} ({}): {} - 输出: {}",
                    r.step.order,
                    tool_name,
                    r.step.step_goal,
                    output.chars().take(50).collect::<String>()
                ))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 添加已执行的步骤记录
    fn push(&mut self, step: PlanStep, duration_ms: u64) {
        self.records.push(StepRecord { step, duration_ms });
    }

    /// 标记步骤为已跳过
    fn mark_skipped(&mut self, order: u8) {
        self.skipped_steps.push(order);
    }
}

// =============================================================================
// PlanExecutor
// =============================================================================

/// 计划执行器
///
/// 按步骤顺序执行计划，支持依赖检查和结果验证
///
/// 探索性步骤采用 Agent 循环模式，内部可多次调用工具直到达成目标。
pub struct PlanExecutor {
    /// 工具执行器
    tool_executor: Arc<dyn ToolExecutor>,
    /// LLM Provider（用于步骤失败时的分析决策）
    llm_provider: Option<Arc<dyn LlmProvider>>,
    /// LLM 模型名称（用于探索性步骤的 Function Calling）
    model: String,
    /// 事件回调（用于实时推送执行事件）
    event_callback: Option<PlanEventCallback>,
    /// 可用工具列表（完整 schema，用于验证工具是否存在和 Function Calling）
    available_tools: Vec<ToolDefinition>,
    /// 步数限制
    max_steps: u8,
    /// 最大重试次数
    max_retries: u8,
    /// 探索性步骤最大工具调用次数（Agent 循环上限）
    max_exploratory_calls: u8,
}

impl PlanExecutor {
    /// 创建新的计划执行器
    pub fn new(tool_executor: Arc<dyn ToolExecutor>) -> Self {
        Self {
            tool_executor,
            llm_provider: None,
            model: String::new(),
            event_callback: None,
            available_tools: Vec::new(),
            max_steps: 50,
            max_retries: 2,
            max_exploratory_calls: 5,  // 探索性步骤最多调用 5 次工具
        }
    }

    /// 设置 LLM Provider（用于步骤失败时的分析决策）
    pub fn with_llm_provider(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.llm_provider = Some(provider);
        self
    }

    /// 设置 LLM 模型名称（用于探索性步骤的 Function Calling）
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 设置探索性步骤最大工具调用次数
    pub fn with_max_exploratory_calls(mut self, max: u8) -> Self {
        self.max_exploratory_calls = max;
        self
    }

    /// 设置可用工具列表
    pub fn with_available_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.available_tools = tools;
        self
    }

    /// 设置事件回调
    pub fn with_event_callback(mut self, callback: PlanEventCallback) -> Self {
        self.event_callback = Some(callback);
        self
    }

    /// 设置最大步骤数
    pub fn with_max_steps(mut self, max: u8) -> Self {
        self.max_steps = max;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max: u8) -> Self {
        self.max_retries = max;
        self
    }

    /// 检查工具是否存在
    fn tool_exists(&self, tool_name: &str) -> bool {
        if self.available_tools.is_empty() {
            // 如果没有配置可用工具列表，默认允许执行
            true
        } else {
            self.available_tools.iter().any(|t| t.function.name == tool_name)
        }
    }

    /// 发送事件到回调
    fn emit(&self, event: PlanStreamEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }

    /// 执行计划
    ///
    /// 接受 `Vec<PlanStep>` 而非 `IntentPlan`：
    /// - `need_agent` 守卫已移除（既然调用本方法即视为 Agent 模式）
    /// - `reasoning` 由调用方在调用前通过 stream_sender 推送给前端（见 `llm_service.rs`），
    ///   本函数不再透传，避免循环依赖
    ///
    /// 执行流程：Goal → Plan → Execute → Observe → Replan → Complete
    pub async fn execute_plan(
        &self,
        steps: Vec<PlanStep>,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<PlanResult, PlanError> {
        info!(
            "PlanExecutor: starting plan with {} steps",
            steps.len(),
        );

        // 空步骤检查
        if steps.is_empty() {
            warn!("PlanExecutor: plan has no steps");
            return Ok(PlanResult {
                completed_steps: 0,
                total_steps: 0,
                final_reply: String::new(),
                step_results: Vec::new(),
                stop_reason: PlanStopReason::Completed,
            });
        }

        // 按 order 排序步骤
        let mut sorted_steps: Vec<PlanStep> = steps;
        sorted_steps.sort_by_key(|s| s.order);

        // 验证步骤数不超过限制
        if sorted_steps.len() > self.max_steps as usize {
            warn!(
                "PlanExecutor: step count {} exceeds max {}, truncating",
                sorted_steps.len(),
                self.max_steps
            );
            sorted_steps.truncate(self.max_steps as usize);
        }

        self.emit(PlanStreamEvent::PlanStart {
            total_steps: sorted_steps.len() as u8,
            reasoning: String::new(),
        });

        // 执行状态
        let mut context = StepContext::new();
        // Plan 级别 LLM 消息上下文（跨探索性步骤累积，由 execute_plan 持有）
        let mut msg_ctx = MessageContext::new(re_act_system_prompt());
        let mut final_reply = String::new();
        let total_steps = sorted_steps.len() as u8;
        let mut stop_reason = PlanStopReason::Completed;

        // =========================================================================
        // 主循环：while 循环 + Observe 决策驱动流控
        // =========================================================================
        let mut idx = 0usize;
        // 当前步骤的重试计数（每次前进到新步骤时重置）
        let mut retry_count: u8 = 0;

        while idx < sorted_steps.len() {
            let step = &sorted_steps[idx];

            // 1. 检查中止标志
            if abort_flag.load(Ordering::SeqCst) {
                stop_reason = PlanStopReason::UserAbort;
                self.emit(PlanStreamEvent::PlanAbort);
                break;
            }

            let step_order = step.order;
            let tool_name = step.actions.first()
                .map(|a: &crate::provider::llm::types::SubAction| a.tool_name.clone())
                .unwrap_or_default();

            // 2. 依赖检查（增强：支持依赖被跳过时的 graceful degradation）
            let mut dep_failed = false;
            for &dep_order in &step.depends_on {
                if context.is_skipped(dep_order) {
                    // 依赖步骤被跳过 → 当前步骤也无法执行，触发 replan 或跳过
                    warn!(
                        "PlanExecutor: step {} depends on skipped step {}, triggering skip",
                        step_order, dep_order
                    );
                    dep_failed = true;
                    break;
                } else if context.get_output(dep_order).is_none() {
                    error!(
                        "PlanExecutor: step {} depends on step {} which was not executed",
                        step_order, dep_order
                    );
                    dep_failed = true;
                    break;
                }
            }

            if dep_failed {
                // 尝试 replan 或 graceful skip
                let skip_reason = format!("Dependency not satisfied for step {}", step_order);
                self.emit(PlanStreamEvent::StepSkipped {
                    step: step_order,
                    reason: skip_reason.clone(),
                });
                context.mark_skipped(step_order);
                msg_ctx.push_step_skipped(step_order, &skip_reason);

                // 如果是关键步骤（最后一步），终止计划
                if step_order == total_steps {
                    stop_reason = PlanStopReason::DependencyFailed;
                    break;
                }
                idx += 1;
                continue;
            }

            // 3. 工具存在性检查（仅确定性步骤需要；Reasoning 不用工具，Exploratory 由 LLM 动态选择）
            if step.step_type == StepType::Deterministic
                && !self.tool_exists(&tool_name)
            {
                warn!(
                    "PlanExecutor: tool '{}' not found (step {}), skipping",
                    tool_name, step_order
                );
                let skip_reason = format!("Tool not found: {}", tool_name);
                self.emit(PlanStreamEvent::StepSkipped {
                    step: step_order,
                    reason: skip_reason.clone(),
                });
                context.mark_skipped(step_order);
                msg_ctx.push_step_skipped(step_order, &skip_reason);

                if step_order == total_steps {
                    stop_reason = PlanStopReason::ToolNotFound;
                    break;
                }
                idx += 1;
                continue;
            }

            // 4. 执行步骤（按 step_type 分发）
            self.emit(PlanStreamEvent::StepStart {
                step: step_order,
                tool: tool_name.clone(),
                goal: step.step_goal.clone(),
            });

            let exec_result = self.execute_single_step(
                step, &context, &mut msg_ctx, abort_flag.clone(),
            ).await;

            // 5. Observe 阶段：评估执行结果，得到 StepObservation
            let observation = match &exec_result {
                Ok(result) => {
                    let output = result.step.actions.last()
                        .and_then(|a| a.output.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let success = !output.is_empty();

                    let summary = if success {
                        format!("输出长度: {} 字符", output.len())
                    } else {
                        "步骤执行完毕但输出为空".to_string()
                    };

                    let decision = if success {
                        ObserveDecision::ContinueNext
                    } else {
                        // 输出为空：临时失败还是永久失败？
                        if retry_count < self.max_retries {
                            ObserveDecision::RetryCurrent
                        } else {
                            ObserveDecision::SkipStep {
                                reason: "步骤输出为空且重试次数已耗尽".to_string(),
                            }
                        }
                    };

                    StepObservation {
                        step_order,
                        success,
                        summary,
                        decision,
                    }
                }
                Err(e) => {
                    let reason = format!("{}", e);
                    let decision = if retry_count < self.max_retries {
                        ObserveDecision::RetryCurrent
                    } else if self.llm_provider.is_some() && idx + 1 < sorted_steps.len() {
                        // 有 LLM 且还有后续步骤，尝试 replan
                        ObserveDecision::ReplanRequired { reason: reason.clone() }
                    } else {
                        ObserveDecision::SkipStep { reason: reason.clone() }
                    };

                    StepObservation {
                        step_order,
                        success: false,
                        summary: reason,
                        decision,
                    }
                }
            };

            // 记录观察到消息上下文
            msg_ctx.push_observation(
                observation.step_order,
                observation.success,
                &observation.summary,
            );

            self.emit(PlanStreamEvent::StepObserved {
                step: observation.step_order,
                success: observation.success,
                summary: observation.summary.clone(),
            });

            // 6. 根据 ObserveDecision 控制流
            match observation.decision {
                ObserveDecision::ContinueNext => {
                    // 提取执行结果（此时 exec_result 一定是 Ok）
                    let result = match exec_result {
                        Ok(r) => r,
                        Err(_) => unreachable!("ContinueNext implies Ok result"),
                    };
                    let duration_ms = result.duration_ms;
                    let executed_step = result.step;

                    let output = executed_step.actions.last()
                        .and_then(|a| a.output.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let output_len = output.len();

                    debug!(
                        "PlanExecutor: step {} completed, output length={}",
                        step_order, output_len
                    );

                    sorted_steps[idx] = executed_step.clone();
                    context.push(executed_step, duration_ms);

                    if step_order == total_steps {
                        final_reply = output;
                    }

                    self.emit(PlanStreamEvent::StepComplete {
                        step: step_order,
                        success: true,
                        duration_ms,
                        output_length: output_len,
                    });

                    retry_count = 0;
                    idx += 1;
                }

                ObserveDecision::RetryCurrent => {
                    retry_count += 1;
                    info!(
                        "PlanExecutor: retrying step {} (attempt {}/{})",
                        step_order, retry_count, self.max_retries
                    );
                    self.emit(PlanStreamEvent::StepRetry {
                        step: step_order,
                        attempt: retry_count,
                        max_retries: self.max_retries,
                    });

                    // 提取上一次尝试的失败原因和已尝试工具，注入消息上下文
                    // 让 LLM 在新一轮决策时明确知道"上次试过什么、为什么失败"
                    let last_reason = observation.summary.clone();
                    let tried_tools: Vec<String> = match &exec_result {
                        Ok(r) => {
                            // 从 SubAction 列表中提取去重后的工具名（保留顺序）
                            let mut seen = std::collections::HashSet::new();
                            r.step.actions.iter()
                                .map(|a| a.tool_name.clone())
                                .filter(|n| seen.insert(n.clone()))
                                .collect()
                        }
                        Err(_) => Vec::new(), // Err 情况下无法获取 actions，留空
                    };
                    msg_ctx.push_retry_attempt(
                        step_order,
                        retry_count + 1, // 1-based: 第 2 次、第 3 次...
                        &last_reason,
                        &tried_tools,
                        None, // 使用默认 hint（领域无关）
                    );

                    // 不推进 idx，重试当前步骤
                }

                ObserveDecision::SkipStep { ref reason } => {
                    warn!(
                        "PlanExecutor: skipping step {}: {}",
                        step_order, reason
                    );
                    self.emit(PlanStreamEvent::StepSkipped {
                        step: step_order,
                        reason: reason.clone(),
                    });
                    context.mark_skipped(step_order);
                    msg_ctx.push_step_skipped(step_order, reason);

                    if step_order == total_steps {
                        stop_reason = PlanStopReason::PartialFailure;
                        break;
                    }

                    retry_count = 0;
                    idx += 1;
                }

                ObserveDecision::ReplanRequired { ref reason } => {
                    info!(
                        "PlanExecutor: replanning from step {}: {}",
                        step_order, reason
                    );

                    let remaining = &sorted_steps[idx + 1..];
                    match self.replan(
                        remaining,
                        &observation.summary,
                        &context,
                        &mut msg_ctx,
                        abort_flag.clone(),
                    ).await {
                        Ok(new_steps) => {
                            info!(
                                "PlanExecutor: replan produced {} new steps",
                                new_steps.len()
                            );

                            // 标记当前步骤为跳过（因为 replan 替换了后续）
                            context.mark_skipped(step_order);

                            // 替换剩余步骤
                            sorted_steps.truncate(idx);
                            sorted_steps.extend(new_steps);

                            self.emit(PlanStreamEvent::PlanReplan {
                                reason: reason.clone(),
                                new_step_count: sorted_steps.len().saturating_sub(idx) as u8,
                            });
                        }
                        Err(e) => {
                            warn!(
                                "PlanExecutor: replan failed: {}, continuing with original plan",
                                e
                            );
                            // replan 失败时 fallback：跳过当前步骤继续
                            context.mark_skipped(step_order);
                            self.emit(PlanStreamEvent::StepSkipped {
                                step: step_order,
                                reason: format!("Replan failed: {}", e),
                            });
                        }
                    }

                    retry_count = 0;
                    idx += 1;
                }

                ObserveDecision::TaskComplete => {
                    info!("PlanExecutor: task marked complete at step {}", step_order);
                    // 如果当前步骤有输出，收集为最终回复
                    if let Ok(result) = exec_result {
                        let output = result.step.actions.last()
                            .and_then(|a| a.output.as_ref())
                            .cloned()
                            .unwrap_or_default();
                        context.push(result.step, result.duration_ms);
                        final_reply = output;
                    }
                    break;
                }
            }
        }

        // =========================================================================
        // 主循环结束：Dump 完整 LLM 消息历史到日志
        // =========================================================================
        // 包含所有步骤的目标、工具结果、观察结论、重试信号、replan 决策等
        // 完整对话上下文，便于事后回溯和审计
        msg_ctx.dump_history(200);

        let completed_steps = context.get_executed_steps().len() as u8;

        self.emit(PlanStreamEvent::PlanComplete {
            completed_steps,
            total_steps,
            final_reply_length: final_reply.len(),
            stop_reason: stop_reason.clone(),
        });

        Ok(PlanResult {
            completed_steps,
            total_steps,
            final_reply,
            step_results: context
                .get_executed_steps()
                .iter()
                .map(|s| {
                    let tool_name = s
                        .actions
                        .first()
                        .map(|a| a.tool_name.clone())
                        .unwrap_or_default();
                    let output = s
                        .actions
                        .last()
                        .and_then(|a| a.output.as_ref())
                        .cloned()
                        .unwrap_or_default();
                    StepResult {
                        order: s.order,
                        tool_name,
                        success: true,
                        output,
                        duration_ms: context.get_duration_ms(s.order),
                    }
                })
                .collect(),
            stop_reason,
        })
    }

    /// 执行单个步骤（按 step_type 分发）
    async fn execute_single_step(
        &self,
        step: &PlanStep,
        context: &StepContext,
        msg_ctx: &mut MessageContext,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<StepExecResult, PlanError> {
        match step.step_type {
            StepType::Deterministic => {
                debug!("确定性步骤：直接执行 {:?}", step);
                execute_deterministic_step(&self.tool_executor, step, context).await
            }
            StepType::Reasoning => {
                debug!("推理性步骤：调用 LLM 生成内容 {:?}", step);
                if self.model.is_empty() {
                    return Err(PlanError::ToolError(ToolExecError {
                        name: "reasoning".to_string(),
                        message: "Reasoning step requires model to be configured".to_string(),
                    }));
                }
                execute_reasoning_step(
                    &self.llm_provider,
                    step,
                    context,
                    &self.model,
                    abort_flag,
                )
                .await
            }
            StepType::Exploratory => {
                debug!("探索性步骤：调用 LLM 决定工具 {:?}", step);
                if self.model.is_empty() {
                    return Err(PlanError::ToolError(ToolExecError {
                        name: "exploratory".to_string(),
                        message: "Exploratory step requires model to be configured".to_string(),
                    }));
                }
                execute_exploratory_step(
                    &self.llm_provider,
                    self.tool_executor.clone(),
                    &self.available_tools,
                    context,
                    msg_ctx,
                    step,
                    abort_flag,
                    &self.model,
                    self.max_exploratory_calls,
                )
                .await
            }
        }
    }

    /// 调用 LLM 重新规划剩余步骤
    async fn replan(
        &self,
        remaining_steps: &[PlanStep],
        observation_summary: &str,
        context: &StepContext,
        msg_ctx: &mut MessageContext,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<Vec<PlanStep>, PlanError> {
        let Some(llm) = &self.llm_provider else {
            return Err(PlanError::ToolError(ToolExecError {
                name: "replan".to_string(),
                message: "Replan requires LLM provider but none configured".to_string(),
            }));
        };

        let history_summary = context.get_history_summary();
        let user_message = build_replan_message(
            remaining_steps,
            observation_summary,
            &history_summary,
        );

        let messages = vec![
            ChatMessage::new(Role::System, replan_system_prompt()),
            ChatMessage::new(Role::User, &user_message),
        ];

        let req = ChatRequest {
            messages,
            model: self.model.clone(),
            temperature: 0.3,
            max_tokens: None,
            tools: None,
        };

        let stream = llm
            .stream_chat(req, abort_flag)
            .await
            .map_err(|e| PlanError::ToolError(ToolExecError {
                name: "replan".to_string(),
                message: format!("LLM stream error: {}", e),
            }))?;

        // 流式收集完整响应
        let mut response = String::new();
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            match item {
                Ok(LlmStreamEvent::TextDelta { text }) => {
                    response.push_str(&text);
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(PlanError::ToolError(ToolExecError {
                        name: "replan".to_string(),
                        message: format!("Stream error: {}", e),
                    }));
                }
            }
        }

        // 解析 LLM 返回的 JSON 步骤列表
        let new_steps = parse_replan_response(&response)
            .map_err(|e| PlanError::ToolError(ToolExecError {
                name: "replan".to_string(),
                message: e,
            }))?;

        // 在消息上下文中记录 replan 决策
        let new_steps_summary = new_steps.iter()
            .map(|s| format!("步骤{} ({}): {}", s.order,
                serde_json::to_string(&s.step_type).unwrap_or_default(),
                s.step_goal))
            .collect::<Vec<_>>()
            .join("\n");
        msg_ctx.push_replan_decision(observation_summary, &new_steps_summary);

        Ok(new_steps)
    }
}

// =============================================================================
// 事件类型（公开 API）
// =============================================================================

/// 计划事件回调类型
pub type PlanEventCallback = Arc<dyn Fn(PlanStreamEvent) + Send + Sync>;

/// 计划执行过程中的流式事件
#[derive(Debug, Clone)]
pub enum PlanStreamEvent {
    /// 计划开始
    PlanStart {
        total_steps: u8,
        reasoning: String,
    },
    /// 步骤开始
    StepStart {
        step: u8,
        tool: String,
        goal: String,
    },
    /// 步骤完成
    StepComplete {
        step: u8,
        success: bool,
        duration_ms: u64,
        output_length: usize,
    },
    /// 步骤错误（不可恢复的致命错误）
    StepError {
        step: u8,
        tool: String,
        error: String,
    },
    /// Observe 阶段：步骤执行后的观察结论
    StepObserved {
        step: u8,
        success: bool,
        summary: String,
    },
    /// 步骤重试
    StepRetry {
        step: u8,
        attempt: u8,
        max_retries: u8,
    },
    /// 步骤被跳过（graceful degradation）
    StepSkipped {
        step: u8,
        reason: String,
    },
    /// 计划中止
    PlanAbort,
    /// 计划 Replan（剩余步骤被重新规划）
    PlanReplan {
        reason: String,
        new_step_count: u8,
    },
    /// 计划完成
    PlanComplete {
        completed_steps: u8,
        total_steps: u8,
        final_reply_length: usize,
        stop_reason: PlanStopReason,
    },
}
