use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub account_id: String,
    pub to: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResponse {
    pub success: bool,
    pub message_id: String,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub account_id: String,
    pub configured: bool,
    pub enabled: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResponse {
    pub success: bool,
    pub account_ids: Vec<String>,
    pub accounts: Vec<AccountInfo>,
    pub count: u32,
    pub running_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrGeneratedData {
    pub qr_data_url: String,
    pub session_key: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedData {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrExpiredData {
    pub retry_count: u32,
    pub max_retries: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedData {
    pub account_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)] 
#[serde(rename_all = "camelCase")]
pub struct LoginSuccessData {
    pub account_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginFailedData {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorData {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "data")]
pub enum SseEvent {
    #[serde(rename = "qr_generated")]
    QrGenerated(QrGeneratedData),
    #[serde(rename = "scanned")]
    Scanned(ScannedData),
    #[serde(rename = "qr_expired")]
    QrExpired(QrExpiredData),
    #[serde(rename = "confirmed")]
    Confirmed(ConfirmedData),
    #[serde(rename = "login_success")]
    LoginSuccess(LoginSuccessData),
    #[serde(rename = "login_failed")]
    LoginFailed(LoginFailedData),
    #[serde(rename = "error")]
    Error(ErrorData),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SseMessage {
    pub event_type: String,
    pub data: String,
}
