//! Planner 模块
//!
//! 提供三层规划架构的实现：
//! - TaskPlannerAgent：用户请求 → TaskStage DAG
//! - DomainRouterAgent：Stage.domain → Planning Rules
//! - ExecutionPlannerAgent：Stage + Rules → ExecutionStep DAG
//!
//! 所有 Agent 共享基础设施 [`agent_base`]。

pub mod agent_base;
pub mod execution_planner_agent;
pub mod task_planner_agent;
