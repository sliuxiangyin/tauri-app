use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::Stream;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::types::ChatRequest;

pub type LlmStream = Pin<Box<dyn Stream<Item = Result<LlmStreamEvent, LlmError>> + Send>>;

/// LLM Provider trait
///
/// 定义统一的 LLM 调用接口，各 Provider 实现此 trait。
///
/// 核心方法：
/// - `send_message`: 非流式调用，返回完整响应
/// - `stream_chat`: 流式调用，实时返回 LlmStreamEvent
///
/// 意图分析能力已迁移到 `agent::IntentAnalyzer`
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 发送消息（非流式）
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError>;

    /// 流式聊天
    async fn stream_chat(
        &self,
        req: ChatRequest,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<LlmStream, LlmError>;
}
