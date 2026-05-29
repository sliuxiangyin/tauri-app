use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::providers::provider_trait::{LlmProvider, LlmStream};
use crate::provider::llm::types::{ChatRequest, ProviderConfigPayload};
use crate::provider::llm::providers::{AnthropicProvider, OllamaProvider, OpenAiCompatible};

pub enum Provider {
    OpenAiCompatible(OpenAiCompatible),
    Anthropic(AnthropicProvider),
    Ollama(OllamaProvider),
}

impl TryFrom<ProviderConfigPayload> for Provider {
    type Error = LlmError;

    fn try_from(value: ProviderConfigPayload) -> Result<Self, Self::Error> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(LlmError::Http)?;

        Ok(match value {
            ProviderConfigPayload::OpenAiCompatible { base_url, api_key } => {
                Self::OpenAiCompatible(OpenAiCompatible::new(base_url, api_key))
            }
            ProviderConfigPayload::Anthropic { api_key } => {
                Self::Anthropic(AnthropicProvider::new(client, api_key))
            }
            ProviderConfigPayload::Ollama { base_url } => {
                Self::Ollama(OllamaProvider::new(client, base_url))
            }
        })
    }
}

#[async_trait]
impl LlmProvider for Provider {
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError> {
        match self {
            Self::OpenAiCompatible(p) => p.send_message(req).await,
            Self::Anthropic(p) => p.send_message(req).await,
            Self::Ollama(p) => p.send_message(req).await,
        }
    }

    async fn stream_chat(
        &self,
        req: ChatRequest,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<LlmStream, LlmError> {
        match self {
            Self::OpenAiCompatible(p) => p.stream_chat(req, abort_flag).await,
            Self::Anthropic(p) => p.stream_chat(req, abort_flag).await,
            Self::Ollama(p) => p.stream_chat(req, abort_flag).await,
        }
    }
}
