use futures_util::StreamExt;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::provider::llm::{
    provider_trait::LlmProvider, types::ChatRequest, types::ProviderConfigPayload,
    LlmChunkEnvelope, LlmStreamEvent, Provider,
};

#[tauri::command]
pub async fn llm_chat_once(
    provider: ProviderConfigPayload,
    req: ChatRequest,
) -> Result<String, String> {
    let p = Provider::try_from(provider).map_err(|e| e.to_string())?;
    p.send_message(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn llm_chat_stream(
    app: AppHandle,
    stream_id: String,
    provider: ProviderConfigPayload,
    req: ChatRequest,
) -> Result<(), String> {
    let stream_id = stream_id.trim();
    if stream_id.is_empty() {
        return Err("stream_id must be non-empty".to_string());
    }
    let stream_id = stream_id.to_string();

    let p = Provider::try_from(provider).map_err(|e| e.to_string())?;
    let mut stream = p.stream_chat(req).await.map_err(|e| e.to_string())?;
    while let Some(item) = stream.next().await {
        match item {
            Ok(ev) => {
                let _ = app.emit("llm:chunk", &LlmChunkEnvelope::new(stream_id.clone(), ev));
            }
            Err(e) => {
                let _ = app.emit(
                    "llm:error",
                    json!({
                        "stream_id": &stream_id,
                        "message": e.to_string(),
                    }),
                );
                let _ = app.emit(
                    "llm:chunk",
                    &LlmChunkEnvelope::new(stream_id.clone(), LlmStreamEvent::Done),
                );
                return Ok(());
            }
        }
    }
    Ok(())
}
