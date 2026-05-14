use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

fn default_temperature() -> f32 {
    1.0
}

/// Payload from the frontend to select a provider and credentials (per request).
///
/// Tauri IPC 对枚举标签使用 `open_ai`（与纯 `serde` 的 `openai_compatible` 不同），
/// 故对 `kind` 与嵌套字段增加 `rename` / `alias`，便于 `invoke` 与手写 JSON 互通。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderConfigPayload {
    #[serde(rename = "open_ai")]
    OpenAiCompatible {
        #[serde(alias = "baseUrl")]
        base_url: String,
        #[serde(alias = "apiKey")]
        api_key: String,
    },
    Anthropic {
        #[serde(alias = "apiKey")]
        api_key: String,
    },
    Ollama {
        #[serde(alias = "baseUrl")]
        base_url: String,
    },
}
