use std::sync::Arc;

use axum::{extract::State, Json};
use log::info;
use tauri_plugin_notification::NotificationExt;

use super::channel::WebhookChannel;
use super::error::ServerError;
use super::types::{SuccessResponse, WebhookPayload};

/// 接收 webhook 推送的消息
/// 通过 broadcast 通道转发给 commands 模块统一处理
/// 同时发送原生系统通知
pub async fn webhook_handler(
    State(channel): State<Arc<WebhookChannel>>,
    Json(payload): Json<WebhookPayload>,
) -> Result<Json<SuccessResponse>, ServerError> {
    let body_preview = if payload.body.len() > 50 {
        format!("{}...", &payload.body[..50])
    } else {
        payload.body.clone()
    };

    info!(
        "收到 webhook 消息 from={} account_id={} body={}",
        payload.from, payload.account_id, body_preview
    );

    // 将消息发送到 broadcast 通道，由 commands 模块统一处理
    let payload_for_channel = payload.clone();
    if let Err(e) = channel.send(payload_for_channel) {
        log::warn!("Webhook 消息发送失败，无活跃消费者: {}", e);
    }

    // 发送原生系统通知
    // 注意：在 axum handler 中无法直接获取 AppHandle，
    // 通知发送通过 commands 模块中的监听任务统一处理
    // 此处仅记录日志，实际通知由 wechat_listen_messages 任务触发
    log::info!("已触发消息通知: from={} body={}", payload.from, body_preview);

    Ok(Json(SuccessResponse::default()))
}

/// 健康检查端点
pub async fn health_handler() -> Result<Json<SuccessResponse>, ServerError> {
    Ok(Json(SuccessResponse {
        success: true,
        message: "service is running".to_string(),
    }))
}
