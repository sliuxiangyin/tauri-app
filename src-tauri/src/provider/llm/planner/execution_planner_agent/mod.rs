//! Execution Planner Agent 模块
//!
//! 在领域规则约束下将 Task Stage 分解为 Execution Step 序列。
//!
//! - [`types`]：ExecutionStep、ExecutionPlan 类型定义
//! - [`prompt`]：EXECUTION_PLANNER_PROMPT 系统提示词常量
//! - [`agent`]：ExecutionPlannerAgent 实现（含流式支持）

pub mod agent;
pub mod prompt;
pub mod types;
