use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::provider::wechat::{
    AccountsResponse, SendMessageRequest, SendMessageResponse, SseEvent, WechatClient,
};
use crate::server::WebhookChannel;

#[tauri::command]
pub async fn wechat_login_stream(
    app: AppHandle,
    account_id: String,
    client: State<'_, WechatClient>,
) -> Result<(), String> {
    let mut stream = client
        .login_stream(&account_id)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(item) = stream.next().await {
        match item {
            Ok(event) => {
                let _ = app.emit("wechat:login_event", &json!(event));
            }
            Err(e) => {
                let _ = app.emit(
                    "wechat:login_error",
                    json!({
                        "account_id": &account_id,
                        "message": e.to_string(),
                    }),
                );
                return Ok(());
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn wechat_send_message(
    req: SendMessageRequest,
    client: State<'_, WechatClient>,
) -> Result<SendMessageResponse, String> {
    client.send_message(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn wechat_get_accounts(
    client: State<'_, WechatClient>,
) -> Result<AccountsResponse, String> {
    client.get_accounts().await.map_err(|e| e.to_string())
}

/// 启动 Webhook 消息监听
/// 后台会持续从 broadcast 通道接收消息，并通过 Tauri 事件推送给前端
/// 此函数可在 setup 中直接调用（传入 AppHandle + Channel），也可作为 command 暴露给前端
pub fn wechat_listen_messages(app: AppHandle, channel: Arc<WebhookChannel>) {
    let mut rx = channel.subscribe();

    tauri::async_runtime::spawn(async move {
        while let Ok(payload) = rx.recv().await {
            println!(
                "[wechat_listen_messages] 转发消息 from={} body={}",
                payload.from, payload.body
            );

            // TODO: 消息落库
            // let db = db_state.get().await?;
            // 使用 sea-orm 实体进行持久化

            // 发送原生系统通知
            let body_preview = if payload.body.len() > 50 {
                format!("{}...", &payload.body[..50])
            } else {
                payload.body.clone()
            };
            let state = app
                .notification()
                .permission_state()
                .unwrap_or(PermissionState::Denied);

            println!("Notification permission state: {:?}", state);

            if state != PermissionState::Granted {
                println!("Permission not granted, skipping notification");
                return;
            }
            
            let _ = app
                .notification()
                .builder()
                .title("收到新消息")
                .body(format!("来自 {}: {}", payload.from, body_preview))
                .show();

            // 通过 Tauri 事件推送给前端
            let _ = app.emit("wechat:message", &json!(payload));
        }
    });
}
