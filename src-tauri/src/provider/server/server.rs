use std::net::SocketAddr;
use std::sync::Arc;

use log::{error, info};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::channel::WebhookChannel;
use super::routes::create_routes;

/// HTTP 服务状态
pub struct HttpServer {
    handle: Mutex<Option<JoinHandle<()>>>,
    port: Mutex<u16>,
}

impl HttpServer {
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
            port: Mutex::new(0),
        }
    }

    /// 启动 HTTP 服务
    /// 端口为 0 时使用随机端口
    #[allow(dead_code)]
    pub async fn start(&self, channel: Arc<WebhookChannel>, port: u16) -> Result<u16, String> {
        let mut handle_guard = self.handle.lock().await;

        if handle_guard.is_some() {
            return Err("HTTP server is already running".to_string());
        }

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("failed to bind to {}: {}", addr, e))?;

        let actual_port = listener
            .local_addr()
            .map_err(|e| format!("failed to get local address: {}", e))?
            .port();

        let app = create_routes(channel);

        let handle = tokio::spawn(async move {
            info!("HTTP server listening on port {}", actual_port);
            if let Err(e) = axum::serve(listener, app).await {
                error!("HTTP server error: {}", e);
            }
        });

        *handle_guard = Some(handle);
        *self.port.lock().await = actual_port;

        info!("HTTP server started on port {}", actual_port);
        Ok(actual_port)
    }

    /// 停止 HTTP 服务
    #[allow(dead_code)]
    pub async fn stop(&self) {
        let mut handle_guard = self.handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
            info!("HTTP server stopped");
        }
        *self.port.lock().await = 0;
    }

    /// 获取当前服务端口
    #[allow(dead_code)]
    pub async fn port(&self) -> u16 {
        *self.port.lock().await
    }

    /// 检查服务是否正在运行
    #[allow(dead_code)]
    pub async fn is_running(&self) -> bool {
        self.handle.lock().await.is_some()
    }
}

impl Default for HttpServer {
    fn default() -> Self {
        Self::new()
    }
}
