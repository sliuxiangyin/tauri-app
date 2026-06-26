use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("HTTP status {status}: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("JSON error: {0}")]
    Json(String),

    #[error("stream ended without assistant content")]
    EmptyResponse,

    #[error("unsupported provider configuration: {0}")]
    Config(String),

    #[error("request timeout after 30 seconds")]
    Timeout,

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

// 自动转换
impl From<reqwest::Error> for LlmError {
    fn from(e: reqwest::Error) -> Self {
        LlmError::Http(e.to_string())
    }
}

impl From<serde_json::Error> for LlmError {
    fn from(e: serde_json::Error) -> Self {
        LlmError::Json(e.to_string())
    }
}

impl Clone for LlmError {
    fn clone(&self) -> Self {
        match self {
            LlmError::Http(s) => LlmError::Http(s.clone()),
            LlmError::HttpStatus { status, body } => LlmError::HttpStatus {
                status: *status,
                body: body.clone(),
            },
            LlmError::Json(s) => LlmError::Json(s.clone()),
            LlmError::EmptyResponse => LlmError::EmptyResponse,
            LlmError::Config(s) => LlmError::Config(s.clone()),
            LlmError::Timeout => LlmError::Timeout,
            LlmError::ParseError(s) => LlmError::ParseError(s.clone()),
            LlmError::NotImplemented(s) => LlmError::NotImplemented(s.clone()),
        }
    }
}
