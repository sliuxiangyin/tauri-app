//! MCP 纯运行时连接管理器
//!
//! 职责：
//! - 管理多个 MCP 服务器的活跃连接（连接池）
//! - 提供连接生命周期操作（connect / disconnect / restart）
//! - 工具调用代理（call_tool / list_tools）
//! - 健康状态查询
//! - 事件推送
//!
//! 不依赖数据库，所有配置由外部调用方传入。

pub mod circuit;
pub mod connection;
pub mod error;
pub mod event;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

pub use connection::{McpConnection, McpStatus, TransportConfig};
pub use error::{McpError, McpResult};
pub use event::{McpEvent, McpEventBus, SharedEventBus};

/// MCP 运行时管理器
///
/// 设计要点：
/// - `connections: RwLock<HashMap<>>` — 读多写少，标准 RwLock 足够
/// - 每个 McpConnection 内部使用 tokio::sync::Mutex 管理 RunningService
/// - 通过 Arc 共享 McpConnection，允许并发工具调用
pub struct McpManager {
    connections: RwLock<HashMap<String, Arc<McpConnection>>>,
    events: SharedEventBus,
    http_client: reqwest::Client,
}

impl McpManager {
    /// 创建新的 MCP 管理器
    pub fn new(http_client: reqwest::Client) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            events: Arc::new(McpEventBus::new()),
            http_client,
        }
    }

    // ─── 生命周期操作 ──────────────────────────────────────

    /// 建立 MCP 连接
    ///
    /// - 如果该名称的连接已存在且活跃，返回 AlreadyConnected 错误
    /// - 如果已存在但断开，则重新连接
    /// - 如果不存在，创建新的 McpConnection 并连接
    pub async fn connect(&self, name: &str, config: TransportConfig) -> McpResult<McpStatus> {
        // 快速检查：是否已存在且活跃
        {
            let map = self.connections.read().unwrap();
            if let Some(conn) = map.get(name) {
                if conn.is_connected() {
                    return Err(McpError::AlreadyConnected {
                        name: name.to_string(),
                    });
                }
                // 存在但断开，释放读锁后在下面重新连接
            }
        }

        // 获取或创建连接
        let conn = {
            let map = self.connections.read().unwrap();
            map.get(name).cloned()
        };

        let conn = match conn {
            Some(conn) => {
                debug!("[McpManager] reusing existing connection entry for '{}'", name);
                conn
            }
            None => {
                let new_conn = Arc::new(McpConnection::new(
                    name.to_string(),
                    config.clone(),
                    self.http_client.clone(),
                    self.events.clone(),
                ));
                let mut map = self.connections.write().unwrap();
                // 双重检查
                if let Some(existing) = map.get(name) {
                    if existing.is_connected() {
                        return Err(McpError::AlreadyConnected {
                            name: name.to_string(),
                        });
                    }
                    existing.clone()
                } else {
                    map.insert(name.to_string(), new_conn.clone());
                    new_conn
                }
            }
        };

        conn.connect().await
    }

    /// 断开 MCP 连接（从池中移除并关闭）
    pub async fn disconnect(&self, name: &str) -> McpResult<McpStatus> {
        let conn = {
            let mut map = self.connections.write().unwrap();
            map.remove(name)
        };

        match conn {
            Some(conn) => {
                info!("[McpManager] disconnecting '{}'", name);
                conn.disconnect().await
            }
            None => Err(McpError::NotFound(name.to_string())),
        }
    }

    /// 重启 MCP 连接（断开 → 重新连接）
    pub async fn restart(&self, name: &str, config: TransportConfig) -> McpResult<McpStatus> {
        // 先断开旧连接（忽略 NotFound 错误）
        let _ = self.disconnect(name).await;

        // 重置熔断器
        {
            let map = self.connections.read().unwrap();
            if let Some(conn) = map.get(name) {
                conn.reset_circuit();
            }
        }

        // 重新连接
        self.connect(name, config).await
    }

    /// 仅从池中移除（不操作数据库，供上层 service 调用）
    pub async fn remove_from_pool(&self, name: &str) -> McpResult<()> {
        let conn = {
            let mut map = self.connections.write().unwrap();
            map.remove(name)
        };

        if let Some(conn) = conn {
            info!("[McpManager] removing '{}' from pool", name);
            conn.disconnect().await?;
        }

        Ok(())
    }

    // ─── 批量初始化 ──────────────────────────────────────

    /// 异步批量连接（后台并发，立即返回）
    ///
    /// 用于启动时初始化所有 enabled 的 MCP 服务。
    /// 每个连接在独立 tokio task 中执行，互不阻塞。
    pub fn connect_batch(self: &Arc<Self>, configs: Vec<(String, TransportConfig)>) {
        for (name, config) in configs {
            let mgr = Arc::clone(self);
            tokio::spawn(async move {
                match mgr.connect(&name, config).await {
                    Ok(status) => {
                        info!(
                            "[McpManager] startup connect '{}' OK (transport={})",
                            name, status.transport_type
                        );
                    }
                    Err(e) => {
                        warn!("[McpManager] startup connect '{}' FAILED: {}", name, e);
                    }
                }
            });
        }
    }

    // ─── 状态查询 ──────────────────────────────────────────

    /// 获取指定连接的状态
    pub fn get_status(&self, name: &str) -> Option<McpStatus> {
        let map = self.connections.read().unwrap();
        map.get(name).map(|conn| conn.get_status())
    }

    /// 获取所有连接的状态
    pub fn list_all_status(&self) -> Vec<McpStatus> {
        let map = self.connections.read().unwrap();
        map.values().map(|conn| conn.get_status()).collect()
    }

    // ─── 工具操作 ──────────────────────────────────────────

    /// 获取指定 MCP 服务的 Tool 列表
    pub async fn get_tools(&self, name: &str) -> McpResult<Vec<Tool>> {
        let conn = self.get_connection(name)?;
        conn.list_tools().await
    }

    /// 调用指定 MCP 服务的 Tool
    ///
    /// 失败时自动判断是否为连接错误，若是则触发重连并重试一次。
    pub async fn call_tool(
        &self,
        name: &str,
        params: CallToolRequestParams,
    ) -> McpResult<CallToolResult> {
        let conn = self.get_connection(name)?;
        conn.call_tool(params).await
    }

    // ─── 事件订阅 ──────────────────────────────────────────

    /// 获取事件订阅器（用于前端监听状态变更）
    pub fn subscribe_events(&self) -> broadcast::Receiver<McpEvent> {
        self.events.subscribe()
    }

    // ─── 内部方法 ──────────────────────────────────────────

    /// 从连接池获取指定连接
    fn get_connection(&self, name: &str) -> McpResult<Arc<McpConnection>> {
        let map = self.connections.read().unwrap();
        map.get(name)
            .cloned()
            .ok_or_else(|| McpError::NotFound(name.to_string()))
    }
}
