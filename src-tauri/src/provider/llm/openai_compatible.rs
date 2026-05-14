use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_openai::Client as OpenAiClient;
use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;

use super::error::LlmError;
use super::provider_trait::{LlmProvider, LlmStream};
use super::stream::LlmStreamEvent;
use super::types::{ChatMessage, ChatRequest, Role};

pub struct OpenAiCompatible {
    client: OpenAiClient<OpenAIConfig>,
}

impl OpenAiCompatible {
    pub fn new(base_url: String, api_key: String) -> Self {
        let config = OpenAIConfig::new()
            .with_api_base(base_url)
            .with_api_key(api_key);
        let client: OpenAiClient<OpenAIConfig> = OpenAiClient::with_config(config);
        Self { client }
    }

    fn convert_messages(
        messages: &[ChatMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>, LlmError> {
        messages
            .iter()
            .map(|m| {
                let msg = match m.role {
                    Role::System => {
                        ChatCompletionRequestSystemMessageArgs::default()
                            .content(&*m.content)
                            .build()
                            .map(ChatCompletionRequestMessage::System)
                            .map_err(|e| LlmError::Config(e.to_string()))?
                    }
                    Role::User => {
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(&*m.content)
                            .build()
                            .map(ChatCompletionRequestMessage::User)
                            .map_err(|e| LlmError::Config(e.to_string()))?
                    }
                    Role::Assistant => {
                        ChatCompletionRequestAssistantMessageArgs::default()
                            .content(&*m.content)
                            .build()
                            .map(ChatCompletionRequestMessage::Assistant)
                            .map_err(|e| LlmError::Config(e.to_string()))?
                    }
                };
                Ok(msg)
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatible {
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError> {
        let messages = Self::convert_messages(&req.messages)?;

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&req.model).messages(messages).temperature(req.temperature);
        if let Some(mt) = req.max_tokens {
            args.max_tokens(mt);
        }
        let request = args
            .build()
            .map_err(|e| LlmError::Config(e.to_string()))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(convert_error)?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        if content.is_empty() {
            return Err(LlmError::EmptyResponse);
        }
        Ok(content)
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<LlmStream, LlmError> {
        let messages = Self::convert_messages(&req.messages)?;

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&req.model).messages(messages).temperature(req.temperature);
        if let Some(mt) = req.max_tokens {
            args.max_tokens(mt);
        }
        let request = args
            .build()
            .map_err(|e| LlmError::Config(e.to_string()))?;

        let stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(convert_error)?;

        let s = try_stream! {
            futures_util::pin_mut!(stream);
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(convert_error)?;
                if let Some(choice) = chunk.choices.first() {
                    if let Some(content) = &choice.delta.content {
                        if !content.is_empty() {
                            yield LlmStreamEvent::TextDelta {
                                text: content.clone(),
                            };
                        }
                    }
                }
            }
            yield LlmStreamEvent::Done;
        };

        Ok(Box::pin(s))
    }
}

fn convert_error(e: async_openai::error::OpenAIError) -> LlmError {
    match e {
        async_openai::error::OpenAIError::Reqwest(http_err) => {
            LlmError::Config(format!("http error: {}", http_err))
        }
        async_openai::error::OpenAIError::ApiError(api_err) => LlmError::HttpStatus {
            status: 0,
            body: api_err.to_string(),
        },
        async_openai::error::OpenAIError::JSONDeserialize(json_err, _body) => {
            LlmError::Json(json_err)
        }
        other => LlmError::Config(other.to_string()),
    }
}
