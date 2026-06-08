//! LLM Provider 实现子模块
//!
//! 包含各厂商适配器和统一的 Provider trait

pub mod anthropic;
pub mod intent_prompt;
pub mod ollama;
pub mod openai_compatible;
pub mod provider_trait;

pub use anthropic::AnthropicProvider;
#[allow(unused_imports)]
pub use intent_prompt::{
    build_intent_user_message, build_tools_description, extract_user_request,
    intent_system_prompt, parse_intent_response,
};
pub use ollama::OllamaProvider;
pub use openai_compatible::OpenAiCompatible;
#[allow(unused_imports)]
pub use provider_trait::{LlmProvider, LlmStream};