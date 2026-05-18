use serde::Serialize;

/// 厂商层解析出的流式片段（不含 `stream_id`），由命令层再包成 [`LlmChunkEnvelope`] 后 `emit`。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LlmStreamEvent {
    TextDelta { text: String },
    Done,
}

/// 流式事件发送端：由调用方创建并传入，供 `stream_chat` 内部转发流式片段。
pub type LlmStreamSender = tokio::sync::mpsc::UnboundedSender<LlmStreamEvent>;

/// 推送到前端的 `llm:chunk` 载荷：固定带 `account_id`，便于 `listen` 里按账号过滤。
#[derive(Debug, Clone, Serialize)]
pub struct LlmChunkEnvelope {
    pub account_id: String,
    #[serde(flatten)]
    pub event: LlmStreamEvent,
}

impl LlmChunkEnvelope {
    pub fn new(account_id: impl Into<String>, event: LlmStreamEvent) -> Self {
        Self {
            account_id: account_id.into(),
            event,
        }
    }
}
