//! MCP 事件系统 - 状态变更推送前端

use std::sync::Arc;
use tokio::sync::broadcast;

/// MCP 服务状态变更事件
#[derive(Debug, Clone)]
pub enum McpEvent {
    /// 连接建立成功
    Connected { name: String },
    /// 连接断开（含原因）
    Disconnected { name: String, reason: String },
    /// 正在重连（含重试次数）
    Reconnecting { name: String, attempt: u32 },
    /// 重连成功
    Reconnected { name: String },
    /// 重连最终失败，熔断器打开
    ReconnectFailed { name: String, error: String },
    /// 服务端通知 tool 列表变更
    ToolsChanged { name: String },
}

/// 事件总线 — 封装 broadcast channel
///
/// 使用 tokio::sync::broadcast 实现多消费者模式：
/// - McpManager 持有 Sender 发布事件
/// - 前端/其他模块通过 Receiver 订阅
#[derive(Clone)]
pub struct McpEventBus {
    tx: broadcast::Sender<McpEvent>,
}

impl McpEventBus {
    /// 创建新的事件总线，缓冲区容量 64
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx }
    }

    /// 发布事件（非阻塞，缓冲区满时丢弃最旧事件）
    pub fn send(&self, event: McpEvent) {
        let _ = self.tx.send(event);
    }

    /// 获取新的订阅器
    pub fn subscribe(&self) -> broadcast::Receiver<McpEvent> {
        self.tx.subscribe()
    }
}

impl Default for McpEventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// McpManager 中使用的事件总线引用
pub type SharedEventBus = Arc<McpEventBus>;
