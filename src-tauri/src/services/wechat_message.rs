//! 微信消息服务层
//!
//! 职责：
//! - 监听 Webhook 通道消息
//! - 消息落库（wechat 渠道）
//! - 调用 LLM 处理微信消息
//! - 回复微信

use crate::services::db::chat::save_message;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::db::DbState;
use crate::entity::chat_message::CreateMessagePayload;
use crate::provider::cache::Cache;
use crate::provider::llm::types::{ChatMessage as LlmChatMessage, Role};
use crate::services::llm_service::{stream_chat, ChatMessage};
use crate::provider::server::WebhookChannel;
use crate::provider::wechat::{SendMessageRequest, WechatClient};

/// 启动微信消息监听服务
/// 在后台持续从 broadcast 通道接收消息，执行以下操作：
/// 1. 消息落库（chat_type='wechat', role='user'）
/// 2. 推送事件到前端
/// 3. 调用 LLM 获取回复
/// 4. LLM 回复落库（chat_type='wechat', role='assistant'）
/// 5. 将 LLM 回复发送到微信
pub async fn start_wechat_message_service(
    app: AppHandle,
    db_state: DbState,
    channel: Arc<WebhookChannel>,
    wechat_client: WechatClient,
    cache: Arc<Cache>,
) {
    let mut rx = channel.subscribe();

    println!("[WechatMessageService] 启动微信消息监听服务");

    while let Ok(payload) = rx.recv().await {
        let account_id = payload.account_id.clone();
        let from = payload.from.clone();
        let body = payload.body.clone();

        println!(
            "[WechatMessageService] 收到消息 from={} body={}",
            from, body
        );

        // 1. 保存微信消息到数据库
        match save_wechat_message(&db_state, &account_id, &body).await {
            Ok(msg) => {
                println!("[WechatMessageService] 消息已落库: id={}", msg.id);
            }
            Err(e) => {
                println!("[WechatMessageService] 消息落库失败: {}", e);
            }
        }

        // 2. 发送原生系统通知
        send_notification(&app, &from, &body);

        // 3. 推送给前端
        let _ = app.emit("wechat:message", &json!(payload));

        // 4. 调用 LLM 获取回复（流式）
        let system_prompt = "你是一个智能助手，请简洁地回复用户的消息。";
        let user_message = ChatMessage {
            role: Role::User,
            content: body.clone(),
        };

        match stream_chat(
            app.clone(),
            cache.clone(),
            &db_state,
            &account_id,
            vec![user_message],
            Some(system_prompt),
        ).await {
            Ok(reply) => {
                println!("[WechatMessageService] LLM 回复: {}", reply);

                // 5. 保存 LLM 回复
                if let Err(e) = save_llm_reply(&db_state, &account_id, &reply).await {
                    println!("[WechatMessageService] LLM 回复落库失败: {}", e);
                }

                // 6. 发送回复到微信
                if let Err(e) =
                    send_reply_to_wechat(&wechat_client, &account_id, &from, &reply).await
                {
                    println!("[WechatMessageService] 发送微信回复失败: {}", e);
                }

                // 7. 推送 LLM 回复到前端
                let _ = app.emit(
                    "wechat:llm_reply",
                    &json!({
                        "account_id": &account_id,
                        "from": &from,
                        "reply": &reply,
                    }),
                );
            }
            Err(e) => {
                println!("[WechatMessageService] LLM 调用失败: {}", e);
            }
        }
    }
}

/// 保存微信消息到数据库
async fn save_wechat_message(
    db_state: &DbState,
    account_id: &str,
    content: &str,
) -> Result<crate::services::db::chat::MessageDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;

    let payload = CreateMessagePayload {
        account_id: account_id.to_string(),
        chat_type: "wechat".to_string(),
        session_id: "default".to_string(),
        role: "user".to_string(),
        content: content.to_string(),
        parent_message_id: None,
        thinking: None,
        tool_calls: None,
        tool_call_id: None,
        tool_output: None,
        extends: Some("{}".to_string()),
        status: Some("completed".to_string()),
        metadata: Some("{}".to_string()),
    };

    save_message(&db, payload).await
}

/// 发送系统通知
fn send_notification(app: &AppHandle, from: &str, body: &str) {
    let body_preview = if body.len() > 50 {
        format!("{}...", &body[..50])
    } else {
        body.to_string()
    };

    let state = app
        .notification()
        .permission_state()
        .unwrap_or(PermissionState::Denied);

    if state == PermissionState::Granted {
        let _ = app
            .notification()
            .builder()
            .title("收到新消息")
            .body(format!("来自 {}: {}", from, body_preview))
            .show();
    }
}

/// 发送回复到微信
async fn send_reply_to_wechat(
    client: &WechatClient,
    account_id: &str,
    to: &str,
    content: &str,
) -> Result<(), String> {
    let req = SendMessageRequest {
        account_id: account_id.to_string(),
        to: to.to_string(),
        text: content.to_string(),
    };

    client.send_message(req).await.map_err(|e| e.to_string())?;

    println!(
        "[WechatMessageService] 已发送微信回复 to={} content={}",
        to, content
    );

    Ok(())
}

/// 保存 LLM 回复到数据库
async fn save_llm_reply(
    db_state: &DbState,
    account_id: &str,
    content: &str,
) -> Result<crate::services::db::chat::MessageDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;

    let payload = CreateMessagePayload {
        account_id: account_id.to_string(),
        chat_type: "wechat".to_string(),
        session_id: "default".to_string(),
        role: "assistant".to_string(),
        content: content.to_string(),
        parent_message_id: None,
        thinking: None,
        tool_calls: None,
        tool_call_id: None,
        tool_output: None,
        extends: Some("{}".to_string()),
        status: Some("completed".to_string()),
        metadata: Some("{}".to_string()),
    };

    save_message(&db, payload).await
}
