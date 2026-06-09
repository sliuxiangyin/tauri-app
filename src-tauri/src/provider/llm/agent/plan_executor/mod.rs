//! 计划执行器
//!
//! 按步骤顺序执行 IntentPlan，支持依赖检查和结果验证
//!
//! 采用混合模式：正常情况下按计划顺序执行；步骤失败时调用 LLM 分析原因并决定后续动作
//!
//! ## 模块结构
//! - `step_executor.rs`：确定性步骤执行（调用工具）
//! - `exploratory_step.rs`：探索性步骤执行（LLM 动态选工具）

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::agent::IntentAnalyzer;
use crate::provider::llm::providers::LlmProvider;
use crate::provider::llm::agent::types::{LlmDecision, StepAction, StepType};
use crate::provider::llm::types::{ChatMessage, FunctionCall, IntentPlan, PlanStep, Role};

pub(crate) mod exploratory_step;
pub(crate) mod step_executor;

use exploratory_step::execute_exploratory_step;
use step_executor::execute_step;

// =============================================================================
// 类型定义
// =============================================================================

/// 计划执行结果
#[derive(Debug, Clone)]
pub struct PlanResult {
    /// 成功执行的步骤数
    pub completed_steps: u8,
    /// 总步骤数
    pub total_steps: u8,
    /// 最终回复内容
    pub final_reply: String,
    /// 各步骤执行摘要
    pub step_results: Vec<StepResult>,
    /// 停止原因
    pub stop_reason: PlanStopReason,
}

/// 单个步骤执行结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepResult {
    /// 步骤序号
    pub order: u8,
    /// 工具名称
    pub tool_name: String,
    /// 是否成功
    pub success: bool,
    /// 执行输出
    pub output: String,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
}

/// 计划停止原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStopReason {
    /// 正常完成
    Completed,
    /// 所有步骤都完成但没有生成回复
    NoFinalReply,
    /// 执行失败（部分步骤失败）
    PartialFailure,
    /// 用户中止
    UserAbort,
    /// 步骤依赖检查失败
    DependencyFailed,
    /// 工具不存在
    ToolNotFound,
}

/// 计划执行错误
#[derive(Debug)]
pub enum PlanError {
    /// 工具执行错误
    ToolError(ToolExecError),
    /// 依赖的前置步骤不存在
    DependencyNotFound(u8),
    /// 工具未找到
    ToolNotFound(String),
    /// 执行被中止
    Aborted,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::ToolError(e) => write!(f, "Tool error: {}", e),
            PlanError::DependencyNotFound(step) => {
                write!(f, "Dependency step {} not found", step)
            }
            PlanError::ToolNotFound(name) => write!(f, "Tool not found: {}", name),
            PlanError::Aborted => write!(f, "Execution aborted by user"),
        }
    }
}

impl std::error::Error for PlanError {}

impl From<ToolExecError> for PlanError {
    fn from(e: ToolExecError) -> Self {
        PlanError::ToolError(e)
    }
}

/// 步骤执行上下文（存储每个步骤的输出，供后续步骤引用）
struct StepContext {
    results: Vec<StepResult>,
}

impl StepContext {
    fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    /// 获取指定步骤的输出
    fn get_output(&self, order: u8) -> Option<&str> {
        self.results
            .iter()
            .find(|r| r.order == order)
            .map(|r| r.output.as_str())
    }
}

// =============================================================================
// PlanExecutor
// =============================================================================

/// 计划执行器
///
/// 按步骤顺序执行计划，支持依赖检查和结果验证
///
/// 采用混合模式：正常情况下按计划顺序执行；步骤失败时调用 LLM 分析原因并决定后续动作
pub struct PlanExecutor {
    /// 工具执行器
    tool_executor: Arc<dyn ToolExecutor>,
    /// LLM Provider（用于步骤失败时的分析决策）
    llm_provider: Option<Arc<dyn LlmProvider>>,
    /// 事件回调（用于实时推送执行事件）
    event_callback: Option<PlanEventCallback>,
    /// 可用工具列表（用于验证工具是否存在）
    available_tools: Vec<String>,
    /// 步数限制
    max_steps: u8,
    /// 最大重试次数
    max_retries: u8,
}

impl PlanExecutor {
    /// 创建新的计划执行器
    pub fn new(tool_executor: Arc<dyn ToolExecutor>) -> Self {
        Self {
            tool_executor,
            llm_provider: None,
            event_callback: None,
            available_tools: Vec::new(),
            max_steps: 50,
            max_retries: 2,
        }
    }

    /// 设置 LLM Provider（用于步骤失败时的分析决策）
    pub fn with_llm_provider(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.llm_provider = Some(provider);
        self
    }

    /// 设置可用工具列表
    pub fn with_available_tools(mut self, tools: Vec<String>) -> Self {
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
            self.available_tools.iter().any(|t| t == tool_name)
        }
    }

    /// 发送事件到回调
    fn emit(&self, event: PlanStreamEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }

    /// 执行计划
    pub async fn execute_plan(
        &self,
        plan: IntentPlan,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<PlanResult, PlanError> {
        info!(
            "PlanExecutor: starting plan with {} steps, need_agent={}",
            plan.steps.len(),
            plan.need_agent
        );

        // 如果不需要 Agent 模式，直接返回空结果
        if !plan.need_agent {
            return Ok(PlanResult {
                completed_steps: 0,
                total_steps: 0,
                final_reply: String::new(),
                step_results: Vec::new(),
                stop_reason: PlanStopReason::Completed,
            });
        }

        // 空步骤检查
        if plan.steps.is_empty() {
            warn!("PlanExecutor: plan has no steps");
            return Ok(PlanResult {
                completed_steps: 0,
                total_steps: 0,
                final_reply: String::new(),
                step_results: Vec::new(),
                stop_reason: PlanStopReason::Completed,
            });
        }

        // 按顺序执行步骤
        let mut context = StepContext::new();
        let mut final_reply = String::new();
        let total_steps = plan.steps.len() as u8;
        let mut stop_reason = PlanStopReason::Completed;

        // 按 order 排序步骤
        let mut sorted_steps = plan.steps.clone();
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
            reasoning: plan.reasoning.clone(),
        });

        for step in &sorted_steps {
            // 检查中止标志
            if abort_flag.load(Ordering::SeqCst) {
                stop_reason = PlanStopReason::UserAbort;
                self.emit(PlanStreamEvent::PlanAbort);
                break;
            }

            // 检查工具是否存在（探索性步骤由 LLM 动态决定工具，跳过此检查）
            if step.step_type != StepType::Exploratory
                && !self.tool_exists(&step.tool_name)
            {
                error!(
                    "PlanExecutor: tool '{}' not found (step {})",
                    step.tool_name, step.order
                );
                self.emit(PlanStreamEvent::StepError {
                    step: step.order,
                    tool: step.tool_name.clone(),
                    error: format!("Tool not found: {}", step.tool_name),
                });
                stop_reason = PlanStopReason::ToolNotFound;
                break;
            }

            // 检查依赖
            if let Some(dep_order) = step.depends_on {
                if let Some(dep_output) = context.get_output(dep_order) {
                    debug!(
                        "PlanExecutor: step {} depends on step {}, output length={}",
                        step.order,
                        dep_order,
                        dep_output.len()
                    );
                } else {
                    error!(
                        "PlanExecutor: step {} depends on step {} which was not executed",
                        step.order, dep_order
                    );
                    self.emit(PlanStreamEvent::StepError {
                        step: step.order,
                        tool: step.tool_name.clone(),
                        error: format!("Dependency step {} not found", dep_order),
                    });
                    stop_reason = PlanStopReason::DependencyFailed;
                    break;
                }
            }

            // 执行步骤（根据 step_type 分发）
            self.emit(PlanStreamEvent::StepStart {
                step: step.order,
                tool: step.tool_name.clone(),
                goal: step.step_goal.clone(),
            });

            let start_time = Instant::now();
            let result = match step.step_type {
                StepType::Deterministic => {
                    tracing::debug!("确定性步骤：直接执行 {:?}", step);
                    execute_step(&self.tool_executor, step, &context).await
                }
                StepType::Exploratory => {
                    tracing::debug!("探索性步骤：调用 LLM 决定工具 {:?}", step);
                    execute_exploratory_step(
                        &self.llm_provider,
                        self.tool_executor.clone(),
                        &self.available_tools,
                        step,
                        &context,
                    )
                    .await
                }
            };
            let duration_ms = start_time.elapsed().as_millis() as u64;

            match result {
                Ok(output) => {
                    let output_len = output.len();
                    debug!(
                        "PlanExecutor: step {} completed, output length={}",
                        step.order,
                        output_len
                    );

                    let step_result = StepResult {
                        order: step.order,
                        tool_name: step.tool_name.clone(),
                        success: true,
                        output: output.clone(),
                        duration_ms,
                    };

                    context.results.push(step_result);

                    // 如果是最后一步，收集输出作为最终回复
                    if step.order == total_steps {
                        final_reply = output;
                    }

                    self.emit(PlanStreamEvent::StepComplete {
                        step: step.order,
                        success: true,
                        duration_ms,
                        output_length: output_len,
                    });
                }
                Err(e) => {
                    error!("PlanExecutor: step {} failed: {}", step.order, e);

                    // 混合模式：尝试让 LLM 分析并决定后续动作
                    let decision = self.handle_step_failure(step, &e, &context).await;

                    match decision {
                        Ok(LlmDecision { analysis, action }) => {
                            info!(
                                "PlanExecutor: LLM decision for step {} - analysis: {}, action: {:?}",
                                step.order, analysis, action
                            );

                            match action {
                                StepAction::Retry { new_parameters } => {
                                    // 重试步骤
                                    let retry_params =
                                        new_parameters.unwrap_or_else(|| step.parameters.clone());
                                    let retry_step = PlanStep {
                                        order: step.order,
                                        step_type: step.step_type,
                                        tool_name: step.tool_name.clone(),
                                        parameters: retry_params,
                                        step_goal: step.step_goal.clone(),
                                        expected_output: step.expected_output.clone(),
                                        depends_on: step.depends_on,
                                    };
                                    match execute_step(
                                        &self.tool_executor,
                                        &retry_step,
                                        &context,
                                    )
                                    .await
                                    {
                                        Ok(output) => {
                                            let step_result = StepResult {
                                                order: step.order,
                                                tool_name: step.tool_name.clone(),
                                                success: true,
                                                output: format!("[重试成功] {}", output),
                                                duration_ms,
                                            };
                                            context.results.pop(); // 移除失败的记录
                                            context.results.push(step_result);
                                            if step.order == total_steps {
                                                final_reply = output.clone();
                                            }
                                            self.emit(PlanStreamEvent::StepComplete {
                                                step: step.order,
                                                success: true,
                                                duration_ms,
                                                output_length: output.len(),
                                            });
                                            continue;
                                        }
                                        Err(_) => {
                                            // 重试也失败，标记为失败
                                            stop_reason = PlanStopReason::PartialFailure;
                                        }
                                    }
                                }
                                StepAction::Skip { reason } => {
                                    // 跳过当前步骤
                                    info!(
                                        "PlanExecutor: skipping step {} - {}",
                                        step.order, reason
                                    );
                                    let step_result = StepResult {
                                        order: step.order,
                                        tool_name: step.tool_name.clone(),
                                        success: false,
                                        output: format!("[已跳过] {}", reason),
                                        duration_ms,
                                    };
                                    context.results.pop();
                                    context.results.push(step_result);
                                    continue;
                                }
                                StepAction::ChangeTool {
                                    tool_name,
                                    parameters,
                                    reason: _,
                                } => {
                                    // 更换工具
                                    let new_step = PlanStep {
                                        order: step.order,
                                        step_type: step.step_type,
                                        tool_name,
                                        parameters,
                                        step_goal: step.step_goal.clone(),
                                        expected_output: step.expected_output.clone(),
                                        depends_on: step.depends_on,
                                    };
                                    match execute_step(
                                        &self.tool_executor,
                                        &new_step,
                                        &context,
                                    )
                                    .await
                                    {
                                        Ok(output) => {
                                            let step_result = StepResult {
                                                order: step.order,
                                                tool_name: new_step.tool_name.clone(),
                                                success: true,
                                                output: format!("[更换工具成功] {}", output),
                                                duration_ms,
                                            };
                                            context.results.pop();
                                            context.results.push(step_result);
                                            if step.order == total_steps {
                                                final_reply = output.clone();
                                            }
                                            self.emit(PlanStreamEvent::StepComplete {
                                                step: step.order,
                                                success: true,
                                                duration_ms,
                                                output_length: output.len(),
                                            });
                                            continue;
                                        }
                                        Err(_) => {
                                            stop_reason = PlanStopReason::PartialFailure;
                                        }
                                    }
                                }
                                StepAction::Abort { reason } => {
                                    // 中止计划
                                    info!("PlanExecutor: aborting plan - {}", reason);
                                    stop_reason = PlanStopReason::PartialFailure;
                                }
                            }
                        }
                        Err(_) => {
                            // 无法获取 LLM 决策，降级为直接失败
                            warn!(
                                "PlanExecutor: LLM decision unavailable, marking as failure"
                            );
                        }
                    }

                    let step_result = StepResult {
                        order: step.order,
                        tool_name: step.tool_name.clone(),
                        success: false,
                        output: format!("Error: {}", e),
                        duration_ms,
                    };

                    context.results.push(step_result);

                    self.emit(PlanStreamEvent::StepError {
                        step: step.order,
                        tool: step.tool_name.clone(),
                        error: e.to_string(),
                    });

                    stop_reason = PlanStopReason::PartialFailure;
                    break;
                }
            }
        }

        let completed_steps = context.results.len() as u8;

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
            step_results: context.results,
            stop_reason,
        })
    }

    /// 处理步骤失败（混合模式核心）
    ///
    /// 当步骤执行失败时，调用 LLM 分析原因并生成决策
    async fn handle_step_failure(
        &self,
        step: &PlanStep,
        error: &PlanError,
        context: &StepContext,
    ) -> Result<LlmDecision, PlanError> {
        // 如果没有配置 LLM Provider，直接返回错误
        let Some(llm) = &self.llm_provider else {
            warn!("PlanExecutor: no LLM provider configured for failure analysis");
            return Err(PlanError::ToolError(ToolExecError {
                name: step.tool_name.clone(),
                message: "LLM provider not available".to_string(),
            }));
        };

        // 构建分析请求的消息
        let error_info = format!(
            "工具: {}\n步骤目标: {}\n参数: {}\n错误: {}",
            step.tool_name,
            step.step_goal,
            serde_json::to_string(&step.parameters).unwrap_or_default(),
            error
        );

        // 构建历史信息
        let history = context
            .results
            .iter()
            .map(|r| {
                format!(
                    "步骤{} ({}): {}",
                    r.order,
                    r.tool_name,
                    if r.success {
                        format!("成功，长度={}", r.output.len())
                    } else {
                        r.output.clone()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let user_message = format!(
            r#"【步骤执行失败】
{error_info}

【已完成步骤】
{history}

【可用工具】
{available_tools}

请分析失败原因，并决定后续动作（JSON 格式）：
{{
    "analysis": "分析说明",
    "action": {{
        "retry": {{"new_parameters": {{"参数": "值"}}}}  | 
        "skip": {{"reason": "跳过原因"}} | 
        "change_tool": {{"tool_name": "新工具名", "parameters": {{}}, "reason": "更换原因"}} | 
        "abort": {{"reason": "中止原因"}}
    }}
}}

注意：如果步骤不是关键的，可以选择 skip；如果参数有问题，可以 retry 并修正参数；如果工具不可用，可以 change_tool；如果问题无法解决，选择 abort。"#,
            error_info = error_info,
            history = if history.is_empty() { "（无）" } else { &history },
            available_tools = self.available_tools.join(", ")
        );

        let messages = vec![
            ChatMessage::new(
                Role::System,
                "你是一个智能助手，负责在工具执行失败时分析原因并决定后续动作。请只返回 JSON，不要其他文字。",
            ),
            ChatMessage::new(Role::User, &user_message),
        ];

        // 调用 LLM 获取决策
        let analyzer = IntentAnalyzer::new(llm.clone());
        let response = analyzer
            .decision_raw(messages, vec![])
            .await
            .map_err(|e| PlanError::ToolError(ToolExecError {
                name: step.tool_name.clone(),
                message: format!("LLM decision failed: {}", e),
            }))?;

        // 解析 LLM 返回的决策
        let decision = self.parse_llm_decision(&response)?;
        Ok(decision)
    }

    /// 解析 LLM 决策响应
    fn parse_llm_decision(&self, response: &str) -> Result<LlmDecision, PlanError> {
        // 提取 JSON（可能有 markdown 格式）
        let json_str = if response.contains('{') {
            let start = response.find('{').unwrap();
            let end = response.rfind('}').unwrap_or(response.len() - 1);
            &response[start..=end]
        } else {
            response
        };

        let json: Value = serde_json::from_str(json_str)
            .map_err(|e| PlanError::ToolError(ToolExecError {
                name: "llm_decision".to_string(),
                message: format!("Failed to parse LLM decision: {}", e),
            }))?;

        let analysis = json
            .get("analysis")
            .and_then(|v| v.as_str())
            .unwrap_or("无分析")
            .to_string();

        let action = json
            .get("action")
            .ok_or_else(|| PlanError::ToolError(ToolExecError {
                name: "llm_decision".to_string(),
                message: "Missing 'action' field in LLM decision".to_string(),
            }))?;

        // 解析动作类型
        if let Some(retry) = action.get("retry") {
            let new_parameters = retry.get("new_parameters").map(|v| v.clone());
            return Ok(LlmDecision::retry(analysis, new_parameters));
        } else if let Some(skip) = action.get("skip") {
            let reason = skip
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("未知原因")
                .to_string();
            return Ok(LlmDecision::skip(reason));
        } else if let Some(change) = action.get("change_tool") {
            let tool_name = change
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let parameters = change
                .get("parameters")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let reason = change
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("工具不可用")
                .to_string();
            return Ok(LlmDecision::change_tool(tool_name, parameters, reason));
        } else if let Some(abort) = action.get("abort") {
            let reason = abort
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("无法解决")
                .to_string();
            return Ok(LlmDecision::abort(reason));
        }

        Err(PlanError::ToolError(ToolExecError {
            name: "llm_decision".to_string(),
            message: "Invalid action type in LLM decision".to_string(),
        }))
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
    /// 步骤错误
    StepError {
        step: u8,
        tool: String,
        error: String,
    },
    /// 计划中止
    PlanAbort,
    /// 计划完成
    PlanComplete {
        completed_steps: u8,
        total_steps: u8,
        final_reply_length: usize,
        stop_reason: PlanStopReason,
    },
}
