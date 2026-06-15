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

impl Provider {
    /// 为 Provider 绑定具体的模型 ID
    ///
    /// 委托给具体子类型的 `with_model` 实现。
    ///
    /// 注: `create_llm_provider` 当前使用内联 match 显式分发,
    /// 暂未调用此方法;保留作为公开 API 以备未来重构/扩展。
    #[allow(dead_code)]
    pub fn with_model(self, model_id: impl Into<String>) -> Self {
        let model_id = model_id.into();
        match self {
            Self::OpenAiCompatible(p) => Self::OpenAiCompatible(p.with_model(model_id)),
            Self::Anthropic(p) => Self::Anthropic(p.with_model(model_id)),
            Self::Ollama(p) => Self::Ollama(p.with_model(model_id)),
        }
    }
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

/// 工厂函数：从 `ProviderConfigPayload` 创建绑定到具体 `model_id` 的 `LlmProvider`
///
/// 内部保留冗长 match 显式分发到三个变体的 `with_model`，
/// 避免依赖 `Provider` 自身的 `with_model` 链式实现，便于未来针对各厂商注入额外配置。
///
/// # 参数
/// - `config`: 数据库存储的厂商配置(OpenAI 兼容 / Anthropic / Ollama)
/// - `model_id`: 用户选择的模型 ID
///
/// # 返回
/// 统一 trait 对象 `Arc<dyn LlmProvider>`,可直接用于 `IntentAnalyzer` 等上层组件
pub fn create_llm_provider(
    config: ProviderConfigPayload,
    model_id: &str,
) -> Result<Arc<dyn LlmProvider>, LlmError> {
    let provider = Provider::try_from(config)?;
    let provider = match provider {
        Provider::OpenAiCompatible(p) => {
            Arc::new(p.with_model(model_id.to_string())) as Arc<dyn LlmProvider>
        }
        Provider::Anthropic(p) => {
            Arc::new(p.with_model(model_id.to_string())) as Arc<dyn LlmProvider>
        }
        Provider::Ollama(p) => {
            Arc::new(p.with_model(model_id.to_string())) as Arc<dyn LlmProvider>
        }
    };
    Ok(provider)
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

    fn default_model(&self) -> Option<&str> {
        match self {
            Self::OpenAiCompatible(p) => p.default_model(),
            Self::Anthropic(p) => p.default_model(),
            Self::Ollama(p) => p.default_model(),
        }
    }
}
