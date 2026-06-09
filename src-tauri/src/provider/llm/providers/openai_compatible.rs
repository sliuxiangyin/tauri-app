use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionTools, CreateChatCompletionRequestArgs,
};
use async_openai::Client as OpenAiClient;
use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::time::timeout;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::providers::provider_trait::{LlmProvider, LlmStream};
use crate::provider::llm::types::{ChatMessage, ChatRequest, Role, ToolDefinition};

pub struct OpenAiCompatible {
    client: OpenAiClient<OpenAIConfig>,
    model: String,
}

impl OpenAiCompatible {
    pub fn new(base_url: String, api_key: String) -> Self {
        let config = OpenAIConfig::new()
            .with_api_base(base_url)
            .with_api_key(api_key);
        let client: OpenAiClient<OpenAIConfig> = OpenAiClient::with_config(config);
        Self { client, model: String::new() }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    #[allow(dead_code)]
    fn model(&self) -> &str {
        &self.model
    }

    fn convert_messages(
        messages: &[ChatMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>, LlmError> {
        messages
            .iter()
            .filter(|m| m.role != Role::Tool) // 过滤掉工具结果消息（已作为独立轮次处理）
            .map(|m| {
                let msg = match m.role {
                    Role::System => ChatCompletionRequestSystemMessageArgs::default()
                        .content(&*m.content)
                        .build()
                        .map(ChatCompletionRequestMessage::System)
                        .map_err(|e| LlmError::Config(e.to_string()))?,
                    Role::User => ChatCompletionRequestUserMessageArgs::default()
                        .content(&*m.content)
                        .build()
                        .map(ChatCompletionRequestMessage::User)
                        .map_err(|e| LlmError::Config(e.to_string()))?,
                    Role::Assistant => {
                        // assistant 消息：如果有 tool_calls 但没有文本，需要一个占位符
                        let content = if !m.content.is_empty() {
                            m.content.clone()
                        } else if m.tool_calls.is_some() {
                            // 有工具调用但没有文本，使用占位符避免 API 错误
                            "[tool_calls]".to_string()
                        } else {
                            String::new()
                        };
                        ChatCompletionRequestAssistantMessageArgs::default()
                            .content(&*content)
                            .build()
                            .map(ChatCompletionRequestMessage::Assistant)
                            .map_err(|e| LlmError::Config(e.to_string()))?
                    }
                    Role::Tool => unreachable!("Tool role should be filtered out above"),
                };
                Ok(msg)
            })
            .collect()
    }

    /// 转换为 OpenAI 特定类型（用于实际 API 调用）
    fn convert_tools_to_provider(
        tools: &[ToolDefinition],
    ) -> Result<Vec<ChatCompletionTools>, LlmError> {
        tools.iter()
            .map(|t| {
                serde_json::to_value(t)
                    .map_err(|e| LlmError::Config(e.to_string()))
                    .and_then(|v| {
                        serde_json::from_value(v)
                            .map_err(|e| LlmError::Config(e.to_string()))
                    })
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatible {
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError> {
        let messages = Self::convert_messages(&req.messages)?;

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&req.model)
            .messages(messages)
            .temperature(req.temperature);
        if let Some(mt) = req.max_tokens {
            args.max_tokens(mt);
        }
        // 注入工具
        if let Some(tools) = &req.tools {
            let provider_tools = Self::convert_tools_to_provider(tools)?;
            args.tools(provider_tools);
        }
        let request = args.build().map_err(|e| LlmError::Config(e.to_string()))?;

        // 发送请求，设置 30 秒超时
        let response = match timeout(Duration::from_secs(30), self.client.chat().create(request)).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => {
                println!("[DEBUG] send_message: API error = {:?}", e);
                return Err(convert_error(e));
            }
            Err(_) => {
                println!("[DEBUG] send_message: TIMEOUT after 30s!");
                return Err(LlmError::Timeout);
            }
        };

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

    async fn stream_chat(
        &self,
        req: ChatRequest,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<LlmStream, LlmError> {
        let messages = Self::convert_messages(&req.messages)?;

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&req.model)
            .messages(messages)
            .temperature(req.temperature);
        if let Some(mt) = req.max_tokens {
            args.max_tokens(mt);
        }
        // 注入工具
        if let Some(tools) = &req.tools {
            let provider_tools = Self::convert_tools_to_provider(tools)?;
            args.tools(provider_tools);
        }
        let request = args.build().map_err(|e| LlmError::Config(e.to_string()))?;

        // 创建流式请求，设置 30 秒超时
        let stream = timeout(
            Duration::from_secs(30),
            self.client.chat().create_stream(request),
        )
        .await
        .map_err(|_| LlmError::Timeout)?
        .map_err(convert_error)?;

        // 用于检测 ToolCallStart：记录已输出的 tool_call index
        let mut emitted_tool_starts: Vec<bool> = Vec::new();
        let s = try_stream! {
            futures_util::pin_mut!(stream);
            loop {
                if abort_flag.load(Ordering::SeqCst) {
                    yield LlmStreamEvent::Done;
                    return;
                }
                match timeout(Duration::from_millis(200), stream.next()).await {
                    Ok(Some(chunk)) => {
                        let chunk = chunk.map_err(convert_error)?;
                        if let Some(choice) = chunk.choices.first() {
                            // 1. TextDelta（普通文本）
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    yield LlmStreamEvent::TextDelta {
                                        text: content.clone(),
                                    };
                                }
                            }

                            // 3. Tool Calls（工具调用）
                            if let Some(tool_calls) = &choice.delta.tool_calls {
                                for tc in tool_calls {
                                    let idx = tc.index as usize;

                                    // 确保 emitted_tool_starts 长度足够
                                    while emitted_tool_starts.len() <= idx {
                                        emitted_tool_starts.push(false);
                                    }

                                    // ToolCallStart：首次出现时（function 首次非空）
                                    if !emitted_tool_starts[idx] {
                                        if let Some(ref func) = tc.function {
                                            if func.name.is_some() {
                                                emitted_tool_starts[idx] = true;
                                                yield LlmStreamEvent::ToolCallStart {
                                                    index: idx as u32,
                                                    id: tc.id.clone().unwrap_or_default(),
                                                    name: func.name.clone().unwrap_or_default(),
                                                };
                                            }
                                        }
                                    }

                                    // ToolCallDelta：参数增量
                                    if let Some(ref func) = tc.function {
                                        if let Some(args) = &func.arguments {
                                            if !args.is_empty() {
                                                yield LlmStreamEvent::ToolCallDelta {
                                                    index: idx as u32,
                                                    arguments: args.clone(),
                                                };
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // 流结束时，为所有已开始的 tool_call 发送 ToolCallDone
                        // 通知上层工具调用参数已完整，可以开始执行
                        for idx in 0..emitted_tool_starts.len() {
                            if emitted_tool_starts[idx] {
                                yield LlmStreamEvent::ToolCallDone {
                                    index: idx as u32,
                                    arguments: Value::Object(serde_json::Map::new()),
                                };
                            }
                        }
                        yield LlmStreamEvent::Done;
                        return;
                    }
                    Err(_) => {
                        // timeout: continue loop and check abort_flag
                    }
                }
            }
        };

        Ok(Box::pin(s))
    }

    fn default_model(&self) -> Option<&str> {
        if self.model.is_empty() { None } else { Some(&self.model) }
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
