//! Task Planner Agent 模块
//!
//! 将用户请求分解为 TaskStage DAG。
//!
//! - [`types`]：TaskStage、TaskPlan、OutputSpec、InputSpec、InputSource 类型定义
//! - [`prompt`]：TASK_PLANNER_PROMPT 系统提示词常量
//! - [`agent`]：TaskPlannerAgent 实现（含流式支持）

pub mod agent;
pub mod prompt;
pub mod types;
