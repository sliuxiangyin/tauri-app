use tokio::sync::broadcast;

use super::types::WebhookPayload;

/// Webhook 消息广播通道
/// 用于将 HTTP server 接收到的消息分发给多个消费者
pub struct WebhookChannel {
    sender: broadcast::Sender<WebhookPayload>,
}

impl WebhookChannel {
    /// 创建新通道，指定消息缓冲区容量
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// 订阅消息通道
    pub fn subscribe(&self) -> broadcast::Receiver<WebhookPayload> {
        self.sender.subscribe()
    }

    /// 发送消息到通道
    pub fn send(
        &self,
        payload: WebhookPayload,
    ) -> Result<usize, broadcast::error::SendError<WebhookPayload>> {
        self.sender.send(payload)
    }
}
