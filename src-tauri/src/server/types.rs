use serde::{Deserialize, Serialize};

/// Webhook 推送的消息体
/// 对应 docs/api.md 中定义的 Webhook 数据格式
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebhookPayload {
    pub body: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "messageSid")]
    pub message_sid: String,
    #[serde(rename = "timestamp")]
    pub timestamp: i64,
    #[serde(rename = "chatType")]
    pub chat_type: String,
    #[serde(rename = "contextToken")]
    pub context_token: String,
    #[serde(rename = "mediaPath")]
    pub media_path: Option<String>,
    #[serde(rename = "mediaType")]
    pub media_type: Option<String>,
    #[serde(rename = "commandBody")]
    pub command_body: String,
    #[serde(rename = "commandAuthorized")]
    pub command_authorized: bool,
}

/// 通用成功响应
#[derive(Debug, serde::Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

impl Default for SuccessResponse {
    fn default() -> Self {
        Self {
            success: true,
            message: "ok".to_string(),
        }
    }
}
