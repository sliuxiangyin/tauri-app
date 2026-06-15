//! Agent 循环子模块
//!
//! 包含 Agent 循环执行器、配置、事件定义和意图分析器
//!
//! ## 模块说明
//! - `analyzer.rs`：意图分析器（仅做意图判断，输出 `need_agent` + `reasoning`）
//! - `config.rs`：AgentConfig
//! - `event.rs`：AgentStreamEvent
//! - `plan_executor/`：计划执行器（目录形式）
//!   - `mod.rs`：PlanExecutor 主结构、execute_plan、StepContext、事件类型
//!   - `step_executor.rs`：确定性步骤执行
//!   - `exploratory_step.rs`：探索性步骤执行（LLM 动态选工具）
//! - `runner.rs`：AgentRunner

pub mod analyzer;
pub mod config;
pub mod event;
pub mod plan_executor;
pub mod runner;
pub mod types;

pub use analyzer::IntentAnalyzer;

pub use config::AgentConfig;
pub use event::{AgentResultSummary, AgentStreamEvent};
pub use plan_executor::{PlanExecutor, PlanEventCallback, PlanResult, PlanStreamEvent, PlanStopReason};
pub use runner::{AgentEventCallback, AgentRunner, parse_mcp_tool_name};
pub use types::{LlmDecision, StepAction, StepType};