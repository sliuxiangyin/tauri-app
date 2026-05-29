use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP status {status}: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("stream ended without assistant content")]
    EmptyResponse,

    #[error("unsupported provider configuration: {0}")]
    Config(String),

    #[error("parse error: {0}")]
    ParseError(String),
}
