use serde::Serialize;

/// 厂商层解析出的流式片段（不含 `stream_id`），由命令层再包成 [`LlmChunkEnvelope`] 后 `emit`。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LlmStreamEvent {
    TextDelta { text: String },
    Done,
}

/// 推送到前端的 `llm:chunk` 载荷：固定带 `stream_id`，便于 `listen` 里过滤并发请求。
#[derive(Debug, Clone, Serialize)]
pub struct LlmChunkEnvelope {
    pub stream_id: String,
    #[serde(flatten)]
    pub event: LlmStreamEvent,
}

impl LlmChunkEnvelope {
    pub fn new(stream_id: impl Into<String>, event: LlmStreamEvent) -> Self {
        Self {
            stream_id: stream_id.into(),
            event,
        }
    }
}
