use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::time::timeout;

use super::error::LlmError;
use super::provider_trait::{LlmProvider, LlmStream};
use super::stream::LlmStreamEvent;
use super::types::{ChatMessage, ChatRequest, Role};

const ANTHROPIC_VERSION: &str = "2023-06-01";

fn anthropic_role(r: Role) -> Result<&'static str, LlmError> {
    match r {
        Role::System => Err(LlmError::Config(
            "Anthropic expects system as top-level field; split in provider".into(),
        )),
        Role::User => Ok("user"),
        Role::Assistant => Ok("assistant"),
    }
}

fn split_system(messages: &[ChatMessage]) -> (Option<String>, Vec<ChatMessage>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut rest: Vec<ChatMessage> = Vec::new();
    for m in messages {
        if m.role == Role::System {
            if !m.content.is_empty() {
                system_parts.push(m.content.clone());
            }
        } else {
            rest.push(m.clone());
        }
    }
    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };
    (system, rest)
}

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(client: reqwest::Client, api_key: String) -> Self {
        Self { client, api_key }
    }

    fn messages_for_api(messages: &[ChatMessage]) -> Result<Vec<Value>, LlmError> {
        let mut out = Vec::new();
        for m in messages {
            let role = anthropic_role(m.role)?;
            out.push(json!({
                "role": role,
                "content": m.content,
            }));
        }
        Ok(out)
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError> {
        let (system, msgs) = split_system(&req.messages);
        let max_tokens = req.max_tokens.unwrap_or(4096);

        let mut body = json!({
            "model": req.model,
            "max_tokens": max_tokens,
            "temperature": req.temperature,
            "messages": Self::messages_for_api(&msgs)?,
        });
        if let Some(s) = system {
            body["system"] = json!(s);
        }

        let res = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            return Err(LlmError::HttpStatus {
                status: status.as_u16(),
                body: text,
            });
        }

        let v: Value = serde_json::from_str(&text)?;
        let content = v["content"][0]["text"].as_str().unwrap_or("").to_string();
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
        let (system, msgs) = split_system(&req.messages);
        let max_tokens = req.max_tokens.unwrap_or(4096);

        let mut body = json!({
            "model": req.model,
            "max_tokens": max_tokens,
            "temperature": req.temperature,
            "messages": Self::messages_for_api(&msgs)?,
            "stream": true,
        });
        if let Some(s) = system {
            body["system"] = json!(s);
        }

        let res = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

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
            futures_util::pin_mut!(bytes_stream);
            loop {
                if abort_flag.load(Ordering::SeqCst) {
                    yield LlmStreamEvent::Done;
                    return;
                }
                match timeout(Duration::from_millis(200), bytes_stream.next()).await {
                    Ok(Some(chunk)) => {
                        let chunk = chunk.map_err(LlmError::Http)?;
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
                            if !line.starts_with("data:") {
                                continue;
                            }
                            let rest = line.strip_prefix("data:").map(str::trim).unwrap_or("");
                            if rest.is_empty() {
                                continue;
                            }
                            let v: Value = match serde_json::from_str(rest) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            let ty = v.get("type").and_then(|t| t.as_str());
                            if ty == Some("content_block_delta") {
                                if let Some(text) = v["delta"]["text"].as_str() {
                                    if !text.is_empty() {
                                        yield LlmStreamEvent::TextDelta {
                                            text: text.to_string(),
                                        };
                                    }
                                }
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
}
