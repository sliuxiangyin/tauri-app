use reqwest::Client;

use super::error::WechatError;
use super::sse::{SseClient, SseStream};
use super::types::{AccountsResponse, SendMessageRequest, SendMessageResponse};

pub struct WechatClient {
    client: Client,
    base_url: String,
    sse_client: SseClient,
}

impl WechatClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::new();
        let sse_client = SseClient::new(client.clone(), base_url.clone());
        Self {
            client,
            base_url,
            sse_client,
        }
    }

    pub fn with_client(client: Client, base_url: String) -> Self {
        let sse_client = SseClient::new(client.clone(), base_url.clone());
        Self {
            client,
            base_url,
            sse_client,
        }
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

    pub async fn login_stream(&self, account_id: &str) -> Result<SseStream, WechatError> {
        self.sse_client.login_stream(account_id).await
    }
}
