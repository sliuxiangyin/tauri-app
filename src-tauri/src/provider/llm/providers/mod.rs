//! LLM Provider 实现子模块
//!
//! 包含各厂商适配器和统一的 Provider trait

pub mod anthropic;
pub mod ollama;
pub mod openai_compatible;
pub mod provider_trait;

pub use anthropic::AnthropicProvider;
pub use ollama::OllamaProvider;
pub use openai_compatible::OpenAiCompatible;
#[allow(unused_imports)]
pub use provider_trait::{LlmProvider, LlmStream};