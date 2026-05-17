use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_stream::try_stream;
use futures_util::{Stream, StreamExt};
use reqwest::Client;

use super::error::WechatError;
use super::types::SseEvent;

pub type SseStream = Pin<Box<dyn Stream<Item = Result<SseEvent, WechatError>> + Send>>;

pub struct SseClient {
    client: Client,
    base_url: String,
}

impl Clone for SseClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
        }
    }
}

impl SseClient {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    pub async fn login_stream(
        &self,
        account_id: &str,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<SseStream, WechatError> {
        let url = format!(
            "{}/login/stream?accountId={}",
            self.base_url.trim_end_matches('/'),
            account_id
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| WechatError::ConnectionFailed(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(WechatError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let bytes_stream = response.bytes_stream();
        let stream = try_stream! {
            let mut buf = String::new();
            let mut current_event = String::new();
            let mut current_data = String::new();
            futures_util::pin_mut!(bytes_stream);
            let mut check_counter = 0u32;

            while let Some(chunk) = bytes_stream.next().await {
                // 检查取消标记（每次循环都检查）
                if abort_flag.load(Ordering::SeqCst) {
                    println!("[SSE] Abort detected at outer loop");
                    break;
                }

                let chunk = chunk?;
                buf.push_str(&String::from_utf8_lossy(&chunk));

                loop {
                    // SSE 解析循环中每处理 4 行检查一次 abort
                    check_counter += 1;
                    if check_counter % 4 == 0 && abort_flag.load(Ordering::SeqCst) {
                        println!("[SSE] Abort detected in inner loop");
                        break;
                    }

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
                        if !current_event.is_empty() && !current_data.is_empty() {
                            let event = parse_sse_event(&current_event, &current_data)?;
                            yield event;
                            current_event.clear();
                            current_data.clear();
                        }
                        continue;
                    }

                    if line.starts_with("event:") {
                        current_event = line
                            .strip_prefix("event:")
                            .map(str::trim)
                            .unwrap_or("")
                            .to_string();
                    } else if line.starts_with("data:") {
                        current_data = line
                            .strip_prefix("data:")
                            .map(str::trim)
                            .unwrap_or("")
                            .to_string();
                    }
                }

                // 如果在内层循环中检测到 abort，退出外层循环
                if abort_flag.load(Ordering::SeqCst) {
                    println!("[SSE] Abort detected after inner loop");
                    break;
                }
            }

            println!("[SSE] Stream ended naturally");
        };

        Ok(Box::pin(stream))
    }
}

fn parse_sse_event(event_type: &str, data: &str) -> Result<SseEvent, WechatError> {
    if data.is_empty() {
        return Err(WechatError::Sse("Empty data".to_string()));
    }

    match event_type {
        "qr_generated" => {
            let data: serde_json::Value = serde_json::from_str(data)?;
            Ok(SseEvent::QrGenerated(super::types::QrGeneratedData {
                qr_data_url: data["qrDataUrl"]
                    .as_str()
                    .ok_or_else(|| WechatError::JsonParse("Missing qrDataUrl".to_string()))?
                    .to_string(),
                session_key: data["sessionKey"]
                    .as_str()
                    .ok_or_else(|| WechatError::JsonParse("Missing sessionKey".to_string()))?
                    .to_string(),
                message: data["message"]
                    .as_str()
                    .unwrap_or("请使用微信扫描二维码")
                    .to_string(),
            }))
        }
        "scanned" => {
            let data: serde_json::Value = serde_json::from_str(data)?;
            Ok(SseEvent::Scanned(super::types::ScannedData {
                message: data["message"]
                    .as_str()
                    .unwrap_or("已扫码，请在微信中确认")
                    .to_string(),
            }))
        }
        "qr_expired" => {
            let data: serde_json::Value = serde_json::from_str(data)?;
            Ok(SseEvent::QrExpired(super::types::QrExpiredData {
                retry_count: data["retryCount"]
                    .as_u64()
                    .ok_or_else(|| WechatError::JsonParse("Missing retryCount".to_string()))?
                    as u32,
                max_retries: data["maxRetries"]
                    .as_u64()
                    .ok_or_else(|| WechatError::JsonParse("Missing maxRetries".to_string()))?
                    as u32,
                message: data["message"]
                    .as_str()
                    .unwrap_or("二维码已过期，正在刷新")
                    .to_string(),
            }))
        }
        "confirmed" => {
            let data: serde_json::Value = serde_json::from_str(data)?;
            Ok(SseEvent::Confirmed(super::types::ConfirmedData {
                account_id: data["accountId"]
                    .as_str()
                    .ok_or_else(|| WechatError::JsonParse("Missing accountId".to_string()))?
                    .to_string(),
                message: data["message"].as_str().unwrap_or("登录已确认").to_string(),
            }))
        }
        "login_success" => {
            let data: serde_json::Value = serde_json::from_str(data)?;
            Ok(SseEvent::LoginSuccess(super::types::LoginSuccessData {
                account_id: data["accountId"]
                    .as_str()
                    .ok_or_else(|| WechatError::JsonParse("Missing accountId".to_string()))?
                    .to_string(),
                message: data["message"].as_str().unwrap_or("登录成功").to_string(),
            }))
        }
        "login_failed" => {
            let data: serde_json::Value = serde_json::from_str(data)?;
            Ok(SseEvent::LoginFailed(super::types::LoginFailedData {
                message: data["message"].as_str().unwrap_or("登录失败").to_string(),
            }))
        }
        "error" => {
            let data: serde_json::Value = serde_json::from_str(data)?;
            Ok(SseEvent::Error(super::types::ErrorData {
                message: data["message"].as_str().unwrap_or("发生错误").to_string(),
            }))
        }
        _ => Err(WechatError::Sse(format!(
            "Unknown event type: {}",
            event_type
        ))),
    }
}
