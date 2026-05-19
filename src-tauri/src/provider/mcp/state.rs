use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::provider::cache::Cache;
use crate::provider::mcp::config::McpServerConfig;
use crate::provider::mcp::error::{is_connection_error, McpError, Result};
use crate::provider::mcp::event::{McpEvent, McpEventSender};
use crate::provider::mcp::manager::{ServerInfo, ToolWithSource};
use crate::provider::mcp::queue::{McpQueue, QueueItem, QueueProcessor};
use crate::provider::mcp::manager::ServerManager;

/// McpState 全局状态（单例）
pub struct McpState {
    /// 队列（并发控制）
    queue: Arc<McpQueue>,
    /// 服务器管理器
    manager: Arc<ServerManager>,
    /// 事件发送器
    event_sender: McpEventSender,
    /// 事件接收器（通过 get_event_receiver() 获取）
    event_receiver: Arc<RwLock<Option<tokio::sync::mpsc::UnboundedReceiver<McpEvent>>>>,
}

impl McpState {
    /// 创建新的 McpState（简化初始化，只需传入 configs 和 cache）
    /// 
    /// # 参数
    /// - `max_concurrency`: 最大并发数，默认 3
    /// - `configs`: 服务器配置列表
    /// - `cache`: 持久化缓存（用于缓存 tools）
    pub fn new(max_concurrency: usize, configs: Vec<McpServerConfig>, cache: Arc<Cache>) -> Self {
        // 创建事件 channel
        let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();

        let (queue, receiver) = McpQueue::new(max_concurrency);
        let queue = Arc::new(queue);
        let manager = Arc::new(ServerManager::new(cache));

        // 创建队列处理器
        let processor = QueueProcessor::new(receiver, queue.clone(), manager.clone());
        processor.set_event_sender(event_sender.clone());

        // 启动处理器
        let processor = Arc::new(RwLock::new(Some(processor)));
        let processor_clone = processor.clone();
        tokio::spawn(async move {
            let mut guard = processor_clone.write().await;
            if let Some(p) = guard.take() {
                p.run().await;
            }
        });

        Self {
            queue,
            manager,
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
        }
    }

    /// 获取事件接收器（用于外部消费事件）
    pub fn get_event_receiver(&self) -> tokio::sync::mpsc::UnboundedReceiver<McpEvent> {
        let mut guard = self.event_receiver.blocking_write();
        guard.take().unwrap_or_else(|| tokio::sync::mpsc::unbounded_channel().1)
    }

    /// 异步初始化（从数据库加载配置后调用）
    pub async fn init(&self, configs: Vec<McpServerConfig>) {
        let count = configs.len();
        let manager = self.manager.clone();
        
        // 收集 server_ids 用于恢复缓存
        let server_ids: Vec<String> = configs.iter().map(|c| c.id.clone()).collect();
        
        // 先恢复缓存
        manager.restore_tools_cache(server_ids).await;
        
        // 再初始化配置
        manager.init(configs).await;
        info!("McpState initialized with {} servers", count);
    }

    /// 内部发送事件的辅助方法
    async fn emit(&self, event: McpEvent) {
        self.event_sender.send(event).ok();
    }

     /// 获取所有服务器配置（快速，无连接检测）
    pub async fn list_configs(&self) -> Vec<McpServerConfig> {
        self.manager.list_configs().await
    }

    /// 获取所有服务器状态（包含 tools）
    pub async fn list_servers(&self) -> Vec<ServerInfo> {
        self.manager.list_servers().await
    }

    /// 手动刷新单个服务器状态（主动检测）
    pub async fn refresh_server(&self, server_id: &str) -> Result<()> {
        // 先停止旧连接
        self.manager.stop_connection(server_id).await?;
        
        // 入队列重新连接
        self.queue.enqueue(QueueItem::Reconnect { id: server_id.to_string() }).await?;
        
        debug!("Refresh requested for server '{}'", server_id);
        Ok(())
    }

    /// 创建新服务器（入队列）
    pub async fn create_server(&self, config: McpServerConfig) -> Result<()> {
        // 检查是否已存在
        if self.manager.get_config(&config.id).await.is_some() {
            return Err(McpError::ServerAlreadyExists {
                server_id: config.id.clone(),
            });
        }

        // 更新配置
        let config_clone = config.clone();
        self.manager.update_config(config.clone()).await;

        // 入队列（事件由 QueueProcessor 发送）
        self.queue.enqueue(QueueItem::Create {
            id: config.id.clone(),
            config,
        }).await?;

        debug!("Create requested for server '{}'", config_clone.id);
        Ok(())
    }

    /// 更新服务器（先停旧服务，再入队列创建）
    pub async fn update_server(&self, id: &str, config: McpServerConfig) -> Result<()> {
        // 检查是否存在
        if self.manager.get_config(id).await.is_none() {
            return Err(McpError::ServerNotFound {
                server_id: id.to_string(),
            });
        }

        // 入队列（先停再创建）
        self.queue.enqueue(QueueItem::Update {
            id: id.to_string(),
            config,
        }).await?;

        debug!("Update requested for server '{}'", id);
        Ok(())
    }

    /// 删除服务器（直接停止并删除）
    pub async fn remove_server(&self, id: &str) -> Result<()> {
        // 入队列删除
        self.queue.enqueue(QueueItem::Remove { id: id.to_string() }).await?;
        
        debug!("Remove requested for server '{}'", id);
        Ok(())
    }

    /// 调用工具（内部检测连接，失败触发重连）
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        match self.manager.call_tool(server_id, tool_name, arguments).await {
            Ok(result) => Ok(result),
            Err(e) if is_connection_error(&e) => {
                // 连接断开，标记状态并触发重连
                let name = self.manager.get_config(server_id)
                    .await
                    .map(|c| c.name)
                    .unwrap_or_else(|| server_id.to_string());

                self.emit(McpEvent::ServerDisconnected {
                    server_id: server_id.to_string(),
                    name: name.clone(),
                    reason: e.to_string(),
                }).await;

                self.manager.update_state(server_id, crate::provider::mcp::manager::ServerState::Disconnected {
                    reason: e.to_string(),
                }).await;

                // 触发重连
                self.queue.enqueue(QueueItem::Reconnect { id: server_id.to_string() }).await?;

                return Err(e);
            }
            Err(e) => Err(e),
        }
    }

    /// 获取所有 Tools
    pub async fn list_tools(&self) -> Vec<ToolWithSource> {
        self.manager.list_tools(None).await.unwrap_or_default()
    }

    /// 获取指定服务器的 Tools
    pub async fn list_server_tools(&self, server_id: &str) -> Result<Vec<ToolWithSource>> {
        self.manager.list_tools(Some(server_id)).await
    }

    /// 检查服务器是否已连接
    pub async fn is_connected(&self, server_id: &str) -> bool {
        self.manager.is_connected(server_id).await
    }

    /// 关闭所有连接
    pub async fn shutdown(&self) {
        self.manager.shutdown().await;
        info!("McpState shutdown complete");
    }
}