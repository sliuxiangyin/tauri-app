use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use reqwest::Client;
use tokio::sync::RwLock;

use super::error::WechatError;
use super::sse::{SseClient, SseStream};
use super::types::{AccountsResponse, SendMessageRequest, SendMessageResponse};

// 登录流状态管理器
struct LoginStreamState {
    abort_flag: Arc<AtomicBool>,
}

pub struct WechatClient {
    client: Client,
    base_url: String,
    sse_client: SseClient,
    // 所有活跃的登录流（使用 RwLock 支持并发访问）
    login_streams: Arc<RwLock<Vec<LoginStreamState>>>,
}

impl Clone for WechatClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            sse_client: self.sse_client.clone(),
            login_streams: self.login_streams.clone(),
        }
    }
}

impl WechatClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::new();
        let sse_client = SseClient::new(client.clone(), base_url.clone());
        Self {
            client,
            base_url,
            sse_client,
            login_streams: Arc::new(RwLock::new(Vec::new())),
        }
    }

    #[allow(dead_code)]
    pub fn with_client(client: Client, base_url: String) -> Self {
        let sse_client = SseClient::new(client.clone(), base_url.clone());
        Self {
            client,
            base_url,
            sse_client,
            login_streams: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 启动登录 SSE 流，返回 (流, 取消标记) 以便命令层也能感知取消信号
    pub async fn login_stream(
        &self,
        account_id: &str,
    ) -> Result<(SseStream, Arc<AtomicBool>), WechatError> {
        // 创建新的 abort flag
        let abort_flag = Arc::new(AtomicBool::new(false));
        println!(
            "[WechatClient] Starting login_stream for account: {}, abort_ptr: {:?}",
            account_id,
            abort_flag.as_ptr()
        );

        // 添加到活跃流列表（Arc::clone 共享同一 AtomicBool）
        let state = LoginStreamState {
            abort_flag: abort_flag.clone(),
        };
        self.login_streams.write().await.push(state);

        // SSE 流也持有同一 abort_flag 的克隆
        let stream = self
            .sse_client
            .login_stream(account_id, abort_flag.clone())
            .await?;
        Ok((stream, abort_flag))
    }

    /// 取消所有活跃的登录流
    pub async fn cancel_login_stream(&self) {
        println!("[WechatClient] Canceling all login_streams");
        let mut guard = self.login_streams.write().await;
        println!("[WechatClient] Active streams count: {}", guard.len());
        for state in guard.iter() {
            println!(
                "[WechatClient] Setting abort_flag to true, ptr: {:?}",
                state.abort_flag.as_ptr()
            );
            state.abort_flag.store(true, Ordering::SeqCst);
        }
        guard.clear();
    }

    pub async fn send_message(
        &self,
        req: SendMessageRequest,
    ) -> Result<SendMessageResponse, WechatError> {
        let url = format!("{}/message/send", self.base_url.trim_end_matches('/'));

        let response = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&req)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(WechatError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let text = response.text().await?;
        tracing::debug!("[WechatClient] send_message response: {}", text);
        let result: SendMessageResponse = serde_json::from_str(&text)?;

        Ok(result)
    }

    pub async fn get_accounts(&self) -> Result<AccountsResponse, WechatError> {
        let url = format!("{}/accounts", self.base_url.trim_end_matches('/'));

        let response = self.client.get(&url).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(WechatError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let text = response.text().await?;
        let result: AccountsResponse = serde_json::from_str(&text)?;

        Ok(result)
    }
}
