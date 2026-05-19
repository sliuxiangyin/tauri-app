use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{debug, info, warn};

use crate::provider::mcp::config::McpServerConfig;
use crate::provider::mcp::error::{McpError, Result};
use crate::provider::mcp::event::{McpEvent, McpEventSender};
use crate::provider::mcp::manager::ServerManager;
use crate::provider::mcp::process::StdioProcessManager;

/// 队列操作类型
#[derive(Debug, Clone)]
pub enum QueueItem {
    /// 创建服务器
    Create { id: String, config: McpServerConfig },
    /// 更新服务器（先停止再创建）
    Update { id: String, config: McpServerConfig },
    /// 重新连接
    Reconnect { id: String },
    /// 停止服务器
    Stop { id: String },
    /// 删除服务器
    Remove { id: String },
}

/// MCP 队列处理器
/// 负责并发控制队列，所有创建/更新操作都通过此队列处理
pub struct McpQueue {
    /// 命令发送通道
    sender: mpsc::Sender<QueueItem>,
    /// 最大并发数
    max_concurrency: usize,
    /// 信号量用于并发控制
    semaphore: Arc<Semaphore>,
    /// 正在处理的操作（用于幂等性检查）
    processing: Arc<RwLock<HashMap<String, VecDeque<QueueItem>>>>,
}

impl McpQueue {
    /// 创建新的 MCP 队列
    pub fn new(max_concurrency: usize) -> (Self, mpsc::Receiver<QueueItem>) {
        let (sender, receiver) = mpsc::channel(100);
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        let queue = Self {
            sender,
            max_concurrency,
            semaphore,
            processing: Arc::new(RwLock::new(HashMap::new())),
        };

        (queue, receiver)
    }

    /// 获取最大并发数
    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    /// 入队操作
    pub async fn enqueue(&self, item: QueueItem) -> Result<()> {
        // 记录正在处理的操作（用于幂等性）
        let server_id = item.server_id().to_string();
        {
            let mut processing = self.processing.write().await;
            processing
                .entry(server_id.clone())
                .or_insert_with(VecDeque::new)
                .push_back(item.clone());
        }

        // 发送到队列
        self.sender
            .send(item)
            .await
            .map_err(|_| McpError::QueueError {
                message: "Failed to send item to queue".to_string(),
            })?;

        debug!("Enqueued operation for server '{}'", server_id);
        Ok(())
    }

    /// 检查服务器是否正在处理中
    pub async fn is_processing(&self, server_id: &str) -> bool {
        let processing = self.processing.read().await;
        processing.contains_key(server_id) && !processing.get(server_id).unwrap().is_empty()
    }

    /// 移除服务器的所有待处理操作
    pub async fn remove_pending(&self, server_id: &str) {
        let mut processing = self.processing.write().await;
        processing.remove(server_id);
    }

    /// 标记操作完成
    pub async fn mark_done(&self, server_id: &str) {
        let mut processing = self.processing.write().await;
        if let Some(queue) = processing.get_mut(server_id) {
            queue.pop_front();
            if queue.is_empty() {
                processing.remove(server_id);
            }
        }
    }

    /// 获取信号量（用于并发控制）
    pub fn semaphore(&self) -> Arc<Semaphore> {
        self.semaphore.clone()
    }
}

impl QueueItem {
    /// 获取关联的服务器 ID
    pub fn server_id(&self) -> &str {
        match self {
            QueueItem::Create { id, .. } => id,
            QueueItem::Update { id, .. } => id,
            QueueItem::Reconnect { id } => id,
            QueueItem::Stop { id } => id,
            QueueItem::Remove { id } => id,
        }
    }

    /// 获取关联的配置（如果有）
    pub fn config(&self) -> Option<&McpServerConfig> {
        match self {
            QueueItem::Create { config, .. } => Some(config),
            QueueItem::Update { config, .. } => Some(config),
            _ => None,
        }
    }
}

/// 队列处理器
/// 在后台运行，处理队列中的操作
pub struct QueueProcessor {
    receiver: mpsc::Receiver<QueueItem>,
    queue: Arc<McpQueue>,
    manager: Arc<ServerManager>,
    event_sender: Arc<RwLock<Option<McpEventSender>>>,
}

impl QueueProcessor {
    /// 创建新的队列处理器
    pub fn new(
        receiver: mpsc::Receiver<QueueItem>,
        queue: Arc<McpQueue>,
        manager: Arc<ServerManager>,
    ) -> Self {
        Self {
            receiver,
            queue,
            manager,
            event_sender: Arc::new(RwLock::new(None)),
        }
    }

    /// 设置事件发送器
    pub fn set_event_sender(&self, sender: McpEventSender) {
        let event_sender = self.event_sender.clone();
        tokio::spawn(async move {
            let mut guard = event_sender.write().await;
            *guard = Some(sender);
        });
    }

    /// 发送事件
    async fn emit(&self, event: McpEvent) {
        let guard = self.event_sender.read().await;
        if let Some(ref sender) = *guard {
            sender.send(event).ok();
        }
    }

    /// 启动处理器（后台任务）
    pub async fn run(mut self) {
        info!("McpQueue processor started");
        
        while let Some(item) = self.receiver.recv().await {
            let server_id = item.server_id().to_string();
            debug!("Processing queue item for server '{}'", server_id);

            // 获取信号量许可（并发控制）
            let semaphore = self.queue.semaphore();
            let permit = semaphore.acquire().await.unwrap();

            // 处理操作
            let result = self.process_item(&item).await;

            // 标记完成
            self.queue.mark_done(&server_id).await;

            // 释放信号量
            drop(permit);

            // 处理结果
            match result {
                Ok(_) => info!("Processed queue item for server '{}'", server_id),
                Err(e) => warn!("Failed to process queue item for server '{}': {}", server_id, e),
            }
        }

        info!("McpQueue processor stopped");
    }

    /// 处理单个队列项
    async fn process_item(&self, item: &QueueItem) -> Result<()> {
        match item {
            QueueItem::Create { id, config } => {
                self.do_create(id, config).await
            }
            QueueItem::Update { id, config } => {
                // 先停止旧的
                self.manager.stop_connection(id).await?;
                // 再创建新的
                self.do_create(id, config).await
            }
            QueueItem::Reconnect { id } => {
                self.do_reconnect(id).await
            }
            QueueItem::Stop { id } => {
                self.manager.stop_connection(id).await?;
                self.emit(McpEvent::ServerStopped {
                    server_id: id.clone(),
                    name: self.manager.get_config(id).await.map(|c| c.name).unwrap_or_else(|| id.clone()),
                }).await;
                Ok(())
            }
            QueueItem::Remove { id } => {
                let name = self.manager.get_config(id).await.map(|c| c.name).unwrap_or_else(|| id.clone());
                self.manager.remove_server(id).await;
                self.queue.remove_pending(id).await;
                self.emit(McpEvent::ServerRemoved {
                    server_id: id.clone(),
                    name,
                }).await;
                Ok(())
            }
        }
    }

    /// 执行创建操作
    async fn do_create(&self, server_id: &str, config: &McpServerConfig) -> Result<()> {
        self.emit(McpEvent::ServerConnecting {
            server_id: server_id.to_string(),
            name: config.name.clone(),
        }).await;

        // 根据传输类型创建连接
        let (running, process_id) = match &config.transport {
            crate::provider::mcp::config::TransportConfig::Stdio { command, args } => {
                // STDIO 类型：检查命令是否可用
                if !StdioProcessManager::check_command(command).await.unwrap_or(false) {
                    self.emit(McpEvent::ServerFailed {
                        server_id: server_id.to_string(),
                        name: config.name.clone(),
                        error: format!("Command '{}' not found or not executable", command),
                    }).await;
                    return Err(McpError::ProcessError {
                        message: format!("Command '{}' not available", command),
                    });
                }

                let transport = StdioProcessManager::build_stdio_transport(command, args).await?;
                let running = rmcp::serve_client((), transport).await.map_err(|e| McpError::ConnectionFailed {
                    message: format!("Failed to create stdio client: {}", e),
                })?;
                // STDIO 连接，进程 ID 由 rmcp 内部管理
                (running, None)
            }
            crate::provider::mcp::config::TransportConfig::Http { url } => {
                let transport = rmcp::transport::StreamableHttpClientTransport::from_uri(url.clone());
                let running = rmcp::serve_client((), transport).await.map_err(|e| McpError::ConnectionFailed {
                    message: format!("Failed to create HTTP client: {}", e),
                })?;
                (running, None)
            }
        };

        // 创建连接
        let connection = crate::provider::mcp::connection::McpConnection::new(
            server_id.to_string(),
            running,
            process_id,
        );

        // 获取工具列表
        let tools = connection.list_tools().await?;
        let tool_count = tools.len();

        // 保存连接和工具
        self.manager.set_connection(server_id.to_string(), connection, tools).await;
        self.manager.update_state(server_id, crate::provider::mcp::manager::ServerState::Connected).await;

        self.emit(McpEvent::ServerConnected {
            server_id: server_id.to_string(),
            name: config.name.clone(),
            tool_count,
        }).await;

        Ok(())
    }

    /// 执行重连操作
    async fn do_reconnect(&self, server_id: &str) -> Result<()> {
        let config = self.manager.get_config(server_id).await
            .ok_or_else(|| McpError::ServerNotFound { server_id: server_id.to_string() })?;

        // 先停止旧的连接
        self.manager.stop_connection(server_id).await?;

        // 重新创建
        self.do_create(server_id, &config).await
    }
}