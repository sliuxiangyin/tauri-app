use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::time::timeout;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::providers::provider_trait::{LlmProvider, LlmStream};
use crate::provider::llm::types::{ChatMessage, ChatRequest, Role, ToolDefinition};

fn join_url(base: &str, path: &str) -> String {
    let b = base.trim_end_matches('/');
    format!("{b}{path}")
}

fn role_str(r: Role) -> &'static str {
    match r {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool", // Ollama 支持 tool role
    }
}

pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[allow(dead_code)]
impl OllamaProvider {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url, model: String::new() }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn messages_json(messages: &[ChatMessage]) -> Vec<Value> {
        messages
            .iter()
            .map(|m| {
                json!({
                    "role": role_str(m.role),
                    "content": m.content,
                })
            })
            .collect()
    }

    fn convert_tools_to_provider(
        tools: &[ToolDefinition],
    ) -> Result<Vec<serde_json::Value>, LlmError> {
        // Ollama 通过 Anthropic 兼容模式支持工具
        // https://docs.ollama.com/api/anthropic-compatibility
        // 格式与 Anthropic 相同
        tools.iter()
            .map(|t| {
                Ok(json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters,
                }))
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError> {
        let url = join_url(&self.base_url, "/api/chat");
        let body = json!({
            "model": req.model,
            "messages": Self::messages_json(&req.messages),
            "stream": false,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            }
        });

        let res = self.client.post(url).json(&body).send().await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            return Err(LlmError::HttpStatus {
                status: status.as_u16(),
                body: text,
            });
        }

        let v: Value = serde_json::from_str(&text)?;
        let content = v["message"]["content"].as_str().unwrap_or("").to_string();
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
        let url = join_url(&self.base_url, "/api/chat");
        let body = json!({
            "model": req.model,
            "messages": Self::messages_json(&req.messages),
            "stream": true,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            }
        });

        let res = self.client.post(url).json(&body).send().await?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(LlmError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let bytes_stream = res.bytes_stream();
        let s = try_stream! {
            let mut buf = String::new();
            let mut prev_assistant = String::new();
            futures_util::pin_mut!(bytes_stream);
            loop {
                if abort_flag.load(Ordering::SeqCst) {
                    yield LlmStreamEvent::Done;
                    return;
                }
                match timeout(Duration::from_millis(200), bytes_stream.next()).await {
                    Ok(Some(chunk)) => {
                        let chunk = chunk.map_err(|e| LlmError::Http(e.to_string()))?;
                        buf.push_str(&String::from_utf8_lossy(&chunk));
                        loop {
                            let idx = match buf.find('\n') {
                                Some(i) => i,
                                None => break,
                            };
                            let mut line = buf[..idx].to_string();
                            if line.ends_with('\r') {
                                line.pop();
                            }
                            buf.drain(..=idx);

                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }
                            let v: Value = match serde_json::from_str(line) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if let Some(role) = v["message"]["role"].as_str() {
                                if role == "assistant" {
                                    if let Some(c) = v["message"]["content"].as_str() {
                                        if c.starts_with(&prev_assistant) {
                                            let delta = &c[prev_assistant.len()..];
                                            prev_assistant = c.to_string();
                                            if !delta.is_empty() {
                                                yield LlmStreamEvent::TextDelta {
                                                    text: delta.to_string(),
                                                };
                                            }
                                        }
                                    }
                                }
                            }
                            if v.get("done") == Some(&json!(true)) {
                                yield LlmStreamEvent::Done;
                                return;
                            }
                        }
                    }
                    Ok(None) => {
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
