//! LLM Provider 模块
//!
//! 统一封装多个 LLM 厂商（OpenAI、Anthropic、Ollama）的聊天补全/流式输出能力。
//!
//! ## 目录结构
//! - `types.rs`：核心类型（ChatMessage, ChatRequest, Role 等）
//! - `error.rs`：错误定义（LlmError）
//! - `llm_event.rs`：LLM 流式事件（LlmStreamEvent, LlmChunkEnvelope）
//! - `dispatcher.rs`：Provider 枚举调度器
//! - `providers/`：Provider 实现子模块
//!   - `provider_trait.rs`：LlmProvider trait
//!   - `openai_compatible.rs`：OpenAI 兼容实现
//!   - `anthropic.rs`：Anthropic 实现
//!   - `ollama.rs`：Ollama 实现
//! - `prompts/`：提示词模板子模块
//!   - `intent_prompt.rs`：意图分析提示词与解析函数
//! - `agent/`：Agent 循环子模块
//!   - `config.rs`：AgentConfig
//!   - `event.rs`：AgentStreamEvent
//!   - `runner.rs`：AgentRunner
//!   - `plan_executor.rs`：计划执行器

pub mod agent;
pub mod content_type_sender;
pub mod dispatcher;
pub mod error;
pub mod llm_event;
pub mod llm_tool_trait;
pub mod ordinary;
pub mod prompts;
pub mod providers;
pub mod types;

#[allow(unused_imports)]
pub use agent::{
    AgentConfig, AgentEventCallback, AgentResultSummary, AgentRunner, AgentStreamEvent,
    parse_mcp_tool_name, PlanExecutor, PlanEventCallback, PlanResult, PlanStreamEvent, PlanStopReason,
    LlmDecision, StepAction, StepType,
    IntentAnalyzer,
};
pub use dispatcher::Provider;
#[allow(unused_imports)]
pub use llm_event::{
    LlmChunkEnvelope, LlmStreamEvent, LlmStreamSender,
};
#[allow(unused_imports)]
pub use llm_tool_trait::{ToolExecutor, ToolExecError, FnToolExecutor};
#[allow(unused_imports)]
pub use ordinary::{process_tool_batch, ProcessResult, ToolExecContext, parse_tool_arguments};
pub use providers::LlmProvider;
#[allow(unused_imports)]
pub use types::{ChatMessage, ChatRequest, IntentPlan, PlanStep};
