//! Planner 模块
//!
//! 提供三层规划架构的实现：
//! - TaskPlannerAgent：用户请求 → TaskStage DAG
//! - DomainRouterAgent：Stage.domain → Planning Rules
//! - ExecutionPlannerAgent：Stage + Rules → ExecutionStep DAG
//! - ReactAgent（占位）：ExecutionStep → 工具调用
//! - PipelineExecutor：串联以上三层，按 DAG 调度执行
//!
//! 所有 Agent 共享基础设施 [`agent_base`]。

pub mod agent_base;
pub mod execution_planner_agent;
pub mod pipeline_executor;
pub mod react_agent;
pub mod task_planner_agent;
