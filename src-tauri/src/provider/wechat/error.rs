use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum WechatError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP status {status}: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JSON parsing error: {0}")]
    JsonParse(String),

    #[error("SSE error: {0}")]
    Sse(String),

    #[error("Empty response")]
    EmptyResponse,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Login failed: {0}")]
    LoginFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}
