use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};

use super::error::LlmError;
use super::provider_trait::{LlmProvider, LlmStream};
use super::stream::LlmStreamEvent;
use super::types::{ChatMessage, ChatRequest, Role};

fn join_url(base: &str, path: &str) -> String {
    let b = base.trim_end_matches('/');
    format!("{b}{path}")
}

fn role_str(r: Role) -> &'static str {
    match r {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

pub struct OpenAiCompatible {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAiCompatible {
    pub fn new(client: reqwest::Client, base_url: String, api_key: String) -> Self {
        Self {
            client,
            base_url,
            api_key,
        }
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
}

#[async_trait]
impl LlmProvider for OpenAiCompatible {
    async fn send_message(&self, req: ChatRequest) -> Result<String, LlmError> {
        let url = join_url(&self.base_url, "/v1/chat/completions");
        let mut body = json!({
            "model": req.model,
            "messages": Self::messages_json(&req.messages),
            "temperature": req.temperature,
            "stream": false,
        });
        if let Some(mt) = req.max_tokens {
            body["max_tokens"] = json!(mt);
        }

        let res = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if content.is_empty() {
            return Err(LlmError::EmptyResponse);
        }
        Ok(content)
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<LlmStream, LlmError> {
        let url = join_url(&self.base_url, "/v1/chat/completions");
        let mut body = json!({
            "model": req.model,
            "messages": Self::messages_json(&req.messages),
            "temperature": req.temperature,
            "stream": true,
        });
        if let Some(mt) = req.max_tokens {
            body["max_tokens"] = json!(mt);
        }

        let res = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
            while let Some(chunk) = bytes_stream.next().await {
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
                    if line == "data: [DONE]" || line == "data:[DONE]" {
                        yield LlmStreamEvent::Done;
                        return;
                    }
                    let rest = line
                        .strip_prefix("data:")
                        .map(str::trim)
                        .unwrap_or("");
                    if rest.is_empty() {
                        continue;
                    }
                    if rest == "[DONE]" {
                        yield LlmStreamEvent::Done;
                        return;
                    }
                    let v: Value = match serde_json::from_str(rest) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if let Some(content) = v["choices"][0]["delta"]["content"].as_str() {
                        if !content.is_empty() {
                            yield LlmStreamEvent::TextDelta {
                                text: content.to_string(),
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
