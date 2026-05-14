use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;

use super::error::LlmError;
use super::stream::LlmStreamEvent;
use super::types::ChatRequest;

pub type LlmStream = Pin<Box<dyn Stream<Item = Result<LlmStreamEvent, LlmError>> + Send>>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError>;

    async fn stream_chat(&self, req: ChatRequest) -> Result<LlmStream, LlmError>;
}
