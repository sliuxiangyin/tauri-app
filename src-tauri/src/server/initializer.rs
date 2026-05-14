use std::sync::Arc;

use tauri::{Emitter, Manager};

use super::{HttpServer, WebhookChannel};

/// 在 Tauri setup 阶段启动 HTTP webhook 服务
/// 创建 broadcast 通道并注册到 Tauri State，供 commands 模块订阅
pub fn start_http_server(app: &mut tauri::App) {
    let app_handle = app.handle().clone();
    let http_server = HttpServer::new();
    let server_port = 8592;

    // 创建 broadcast 通道并注册到 Tauri State
    let channel = Arc::new(WebhookChannel::new(100));
    app.manage(channel.clone());

    tauri::async_runtime::spawn(async move {
        match http_server.start(channel, server_port).await {
            Ok(port) => {
                let _ = app_handle.emit("server:started", serde_json::json!({ "port": port }));
            }
            Err(e) => {
                log::error!("Failed to start HTTP server: {}", e);
            }
        }
    });
}
