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

use tracing::{debug, error, info, warn};

use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::providers::LlmProvider;
use crate::provider::llm::agent::types::StepType;
use crate::provider::llm::types::{IntentPlan, PlanStep, ToolDefinition};

pub(crate) mod exploratory_step;
pub(crate) mod message_context;
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

/// 步骤执行上下文（存储已执行的步骤，供后续步骤引用）
pub(crate) struct StepContext {
    /// 已执行的步骤列表
    executed_steps: Vec<PlanStep>,
}

impl StepContext {
    fn new() -> Self {
        Self {
            executed_steps: Vec::new(),
        }
    }

    /// 获取指定步骤的输出
    fn get_output(&self, order: u8) -> Option<String> {
        self.executed_steps
            .iter()
            .find(|s| s.order == order)
            .and_then(|s| s.actions.last())  // 取最后一个 SubAction 的输出
            .and_then(|a| a.output.as_ref())
            .map(|s| s.clone())
    }

    /// 获取已执行的步骤列表
    fn get_executed_steps(&self) -> &[PlanStep] {
        &self.executed_steps
    }

    /// 获取历史摘要（用于 LLM 上下文）
    fn get_history_summary(&self) -> String {
        if self.executed_steps.is_empty() {
            return String::new();
        }

        self.executed_steps
            .iter()
            .filter_map(|s| {
                // 从 actions 列表获取输出
                let output = s
                    .actions
                    .last()
                    .and_then(|a| a.output.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or("（无输出）");

                // 获取第一个动作的工具名
                let tool_name = s
                    .actions
                    .first()
                    .map(|a| a.tool_name.as_str())
                    .unwrap_or("（无工具）");

                Some(format!(
                    "步骤{} ({}): {} - 输出: {}",
                    s.order,
                    tool_name,
                    s.step_goal,
                    output.chars().take(50).collect::<String>()
                ))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 添加已执行的步骤
    fn push(&mut self, step: PlanStep) {
        self.executed_steps.push(step);
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

        // 按 order 排序步骤
        let mut sorted_steps: Vec<PlanStep> = plan.steps.clone();
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

        // 按顺序执行步骤
        let mut context = StepContext::new();
        let mut final_reply = String::new();
        let total_steps = sorted_steps.len() as u8;
        let mut stop_reason = PlanStopReason::Completed;

        for idx in 0..sorted_steps.len() {
            let step = &sorted_steps[idx];
            // 检查中止标志
            if abort_flag.load(Ordering::SeqCst) {
                stop_reason = PlanStopReason::UserAbort;
                self.emit(PlanStreamEvent::PlanAbort);
                break;
            }

            // 检查工具是否存在（探索性步骤由 LLM 动态决定工具，跳过此检查）
            let tool_name = step.actions.first()
                .map(|a| a.tool_name.clone())
                .unwrap_or_default();
            if step.step_type != StepType::Exploratory
                && !self.tool_exists(&tool_name)
            {
                error!(
                    "PlanExecutor: tool '{}' not found (step {})",
                    tool_name, step.order
                );
                self.emit(PlanStreamEvent::StepError {
                    step: step.order,
                    tool: tool_name.clone(),
                    error: format!("Tool not found: {}", tool_name),
                });
                stop_reason = PlanStopReason::ToolNotFound;
                break;
            }

            // 检查依赖
            for &dep_order in &step.depends_on {
                if context.get_output(dep_order).is_some() {
                    debug!(
                        "PlanExecutor: step {} depends on step {}, output available",
                        step.order, dep_order
                    );
                } else {
                    error!(
                        "PlanExecutor: step {} depends on step {} which was not executed",
                        step.order, dep_order
                    );
                    self.emit(PlanStreamEvent::StepError {
                        step: step.order,
                        tool: tool_name.clone(),
                        error: format!("Dependency step {} not found", dep_order),
                    });
                    stop_reason = PlanStopReason::DependencyFailed;
                    break;
                }
            }
            if matches!(stop_reason, PlanStopReason::DependencyFailed) {
                break;
            }

            // 执行步骤（根据 step_type 分发）
            self.emit(PlanStreamEvent::StepStart {
                step: step.order,
                tool: tool_name,
                goal: step.step_goal.clone(),
            });

            let step_order = step.order;
            let start_time = Instant::now();

            // 根据步骤类型执行
            let executed_step: PlanStep = match step.step_type {
                StepType::Deterministic => {
                    tracing::debug!("确定性步骤：直接执行 {:?}", step);
                    execute_step(&self.tool_executor, step, &context).await?
                }
                StepType::Reasoning => {
                    tracing::debug!("推理性步骤：调用 LLM 生成内容 {:?}", step);
                    // Reasoning 步骤由 LLM 推理生成内容，无工具调用
                    // TODO: 实现 reasoning 专用执行逻辑（调用 LLM 生成文本内容）
                    execute_step(&self.tool_executor, step, &context).await?
                }
                StepType::Exploratory => {
                    tracing::debug!("探索性步骤：调用 LLM 决定工具 {:?}", step);

                    // 检查是否配置了 model
                    if self.model.is_empty() {
                        return Err(PlanError::ToolError(ToolExecError {
                            name: "exploratory".to_string(),
                            message: "Exploratory step requires model to be configured".to_string(),
                        }));
                    }

                    // 传入 context（包含历史记录）用于构建上下文
                    execute_exploratory_step(
                        &self.llm_provider,
                        self.tool_executor.clone(),
                        &self.available_tools,
                        &context,
                        step,
                        abort_flag.clone(),
                        &self.model,
                        self.max_exploratory_calls,
                    )
                    .await?
                }
            };

            let duration_ms = start_time.elapsed().as_millis() as u64;

            // 从 executed_step.actions 提取最后一个 output
            let output = executed_step
                .actions
                .last()
                .and_then(|a| a.output.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            let output_len = output.len();
            let tool_name = executed_step
                .actions
                .first()
                .map(|a| a.tool_name.as_str())
                .unwrap_or("（无工具）")
                .to_string();

            debug!(
                "PlanExecutor: step {} completed, tool={}, output length={}",
                executed_step.order,
                tool_name,
                output_len
            );

            // 更新 sorted_steps
            sorted_steps[idx] = executed_step.clone();
            context.push(executed_step.clone());

            // 如果是最后一步，收集输出作为最终回复
            if step_order == total_steps {
                final_reply = output;
            }

            self.emit(PlanStreamEvent::StepComplete {
                step: step_order,
                success: true,
                duration_ms,
                output_length: output_len,
            });
        }

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
                        .map(|o| o.clone())
                        .unwrap_or_default();
                    StepResult {
                        order: s.order,
                        tool_name,
                        success: true,
                        output,
                        duration_ms: 0, // 不再追踪 duration_ms
                    }
                })
                .collect(),
            stop_reason,
        })
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
