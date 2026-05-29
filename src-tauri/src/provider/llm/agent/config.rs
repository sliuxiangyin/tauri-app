//! Agent 循环配置参数

use std::time::Duration;

/// Agent 循环控制配置
///
/// 用于控制 Agent 循环的执行策略和终止条件。
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// 最大迭代次数，防止无限循环
    pub max_steps: u32,
    /// 每步超时时间（LLM 调用 + 工具执行）
    pub timeout_per_step: Duration,
    /// 总超时时间（整个 Agent 执行周期）
    pub timeout_total: Duration,
    /// 无效响应阈值：连续多少次空响应后终止
    pub empty_response_threshold: u32,
    /// 连续错误阈值：超过后终止
    pub error_threshold: u32,
    /// 空内容时是否继续（true=继续，false=终止）
    pub continue_on_empty: bool,
    /// 工具执行失败时是否继续（true=继续，false=终止）
    pub continue_on_tool_error: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            timeout_per_step: Duration::from_secs(60),
            timeout_total: Duration::from_secs(300),
            empty_response_threshold: 3,
            error_threshold: 5,
            continue_on_empty: false,
            continue_on_tool_error: true,  // 工具失败不终止，允许 LLM 自行处理
        }
    }
}

impl AgentConfig {
    /// 创建自定义配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最大迭代次数
    pub fn with_max_steps(mut self, steps: u32) -> Self {
        self.max_steps = steps;
        self
    }

    /// 设置每步超时
    pub fn with_timeout_per_step(mut self, timeout: Duration) -> Self {
        self.timeout_per_step = timeout;
        self
    }

    /// 设置总超时
    pub fn with_timeout_total(mut self, timeout: Duration) -> Self {
        self.timeout_total = timeout;
        self
    }

    /// 设置空响应阈值
    pub fn with_empty_threshold(mut self, threshold: u32) -> Self {
        self.empty_response_threshold = threshold;
        self
    }

    /// 设置错误阈值
    pub fn with_error_threshold(mut self, threshold: u32) -> Self {
        self.error_threshold = threshold;
        self
    }

    /// 空内容时继续执行
    pub fn continue_on_empty(mut self, continue_on: bool) -> Self {
        self.continue_on_empty = continue_on;
        self
    }

    /// 工具失败时继续执行
    pub fn continue_on_tool_error(mut self, continue_on: bool) -> Self {
        self.continue_on_tool_error = continue_on;
        self
    }
}