//! Agent 循环子模块
//!
//! 包含 Agent 循环执行器、配置、事件定义和意图分析器
//!
//! ## 模块说明
//! - `analyzer.rs`：意图分析器（统一意图分析和决策能力）
//! - `config.rs`：AgentConfig
//! - `event.rs`：AgentStreamEvent
//! - `plan_executor.rs`：计划执行器
//! - `runner.rs`：AgentRunner

pub mod analyzer;
pub mod config;
pub mod event;
pub mod plan_executor;
pub mod runner;
pub mod types;

pub use analyzer::{IntentAnalyzer, provider_helper};

pub use config::AgentConfig;
pub use event::{AgentResultSummary, AgentStreamEvent};
pub use plan_executor::{PlanExecutor, PlanEventCallback, PlanResult, PlanStreamEvent, PlanStopReason};
pub use runner::{AgentEventCallback, AgentRunner, parse_mcp_tool_name};
pub use types::{LlmDecision, StepAction, StepType};