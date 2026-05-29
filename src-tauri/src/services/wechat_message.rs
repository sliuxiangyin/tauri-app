//! 微信消息服务层
//!
//! 职责：
//! - 监听 Webhook 通道消息
//! - 消息落库（wechat 渠道）
//! - 调用 LLM 处理微信消息
//! - 回复微信
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::{NotificationExt, PermissionState};
use tracing::{error, info, warn};

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::llm::types::Role;
use crate::provider::mcp::McpManager;
use crate::services::llm_service::{self, ChatMessage};
use crate::provider::server::WebhookChannel;
use crate::provider::wechat::{WechatClient, SendMessageRequest};
use crate::types::chat::ChatContext;

/// 启动微信消息监听服务
///
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
    _cache: Arc<Cache>,
    mcp_manager: Arc<McpManager>,
) {
    let mut rx = channel.subscribe();

    info!("[WechatMessageService] 启动微信消息监听服务");

    // 克隆 Arc 用于循环中传入（每次迭代需要所有权）
    let mcp_manager_inner = mcp_manager.clone();

    while let Ok(payload) = rx.recv().await {
        let account_id = payload.account_id.clone();
        let from = payload.from.clone();
        let body = payload.body.clone();

        // 2. 发送原生系统通知
        send_notification(&app, &from, &body);
        // 3. 推送给前端
        let _ = app.emit("wechat:message", &json!(payload));

        // 4. 组装 ChatContext 并调用统一 LLM 入口
        let ctx = ChatContext {
            account_id: account_id.clone(),
            chat_type: "wechat".to_string(),
            session_id: "default".to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: body.clone(),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }],
        };

        let (reply, status) = match llm_service::chat_with_placeholder(
            &db_state,
            mcp_manager_inner.clone(),
            ctx,
            None, // 微信渠道不需要流式推送，只关心最终结果
            Arc::new(AtomicBool::new(false)), // 微信渠道不需要取消
        ).await {
            Ok(reply) => {
                (reply, "success")
            }
            Err(e) => {
                error!("[WechatMessageService] LLM 调用失败: {}", e);
                let fallback = format!("抱歉，服务暂时不可用，请稍后再试。错误: {}", e);
                (fallback, "error")
            }
        };

        // 5. 发送回复到微信（无论 LLM 成功或失败都发送）
        if let Err(e) = send_reply_to_wechat(&wechat_client, &account_id, &from, &reply).await {
            warn!("[WechatMessageService] 发送微信回复失败: {}", e);
        }

        // 6. 推送 LLM 回复到前端
        let _ = app.emit(
            "wechat:llm_reply",
            &json!({
                "account_id": &account_id,
                "from": &from,
                "reply": &reply,
                "status": status,
            }),
        );
    }
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

    Ok(())
}

