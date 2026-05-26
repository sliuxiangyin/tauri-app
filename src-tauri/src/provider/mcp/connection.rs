//! MCP 单连接生命周期管理
//!
//! 每个 McpConnection 管理一个 MCP 服务器的完整生命周期：
//! 连接建立 → 工具调用 → 健康检测 → 断开清理 → 自动重连

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::circuit::{CircuitBreaker, CircuitBreakerConfig};
use super::error::{McpError, McpResult};
use super::event::{McpEvent, SharedEventBus};

/// 传输层配置（由外部调用方传入）
#[derive(Debug, Clone)]
pub enum TransportConfig {
    /// STDIO 模式：启动子进程，通过标准输入/输出通信
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    /// HTTP Streamable 模式：通过 HTTP + SSE 通信
    Http {
        url: String,
    },
}

impl TransportConfig {
    /// 解析 transport 类型标签
    pub fn transport_type(&self) -> &str {
        match self {
            TransportConfig::Stdio { .. } => "stdio",
            TransportConfig::Http { .. } => "http",
        }
    }
}

/// 连接健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionHealth {
    Connected,
    Disconnected,
    Reconnecting,
    Failed,
}

/// MCP 服务运行时状态（返回给调用方）
#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    pub name: String,
    pub transport_type: String,
    pub health: String,
    pub circuit_open: bool,
    pub fail_count: u32,
}

/// 重连策略配置
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// 初始退避时间
    pub initial_backoff: Duration,
    /// 最大退避时间
    pub max_backoff: Duration,
    /// 退避乘数
    pub backoff_multiplier: f64,
    /// 最大重试次数（None = 无限）
    pub max_retries: Option<u32>,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_retries: Some(5),
        }
    }
}

/// MCP 单连接管理器
///
/// 设计要点：
/// - `service: Mutex<Option<RunningService>>` — tokio async Mutex，允许跨 await 持锁
/// - `RunningService` 实现了 `Deref<Target = Peer<RoleClient>>`，可以直接调用 `call_tool` 等方法
/// - `close()` 需要 `&mut self`，通过 Mutex 取出后 drop guard 再关闭
pub struct McpConnection {
    /// 服务名称（唯一标识）
    pub name: String,
    /// 传输层配置
    config: TransportConfig,
    /// 活跃的 MCP 客户端连接
    service: Mutex<Option<RunningService<RoleClient, ()>>>,
    /// 熔断器
    circuit: CircuitBreaker,
    /// 连接是否健康（轻量原子标记，避免每次加锁）
    connected: AtomicBool,
    /// 重连配置
    reconnect_config: ReconnectConfig,
    /// 事件总线（发布状态变更）
    events: SharedEventBus,
    /// 共享 HTTP 客户端
    http_client: reqwest::Client,
}

impl McpConnection {
    /// 创建新的连接管理器（初始状态：未连接）
    pub fn new(
        name: String,
        config: TransportConfig,
        http_client: reqwest::Client,
        events: SharedEventBus,
    ) -> Self {
        Self {
            name,
            config,
            service: Mutex::new(None),
            circuit: CircuitBreaker::new(CircuitBreakerConfig::default()),
            connected: AtomicBool::new(false),
            reconnect_config: ReconnectConfig::default(),
            events,
            http_client,
        }
    }

    /// 建立 MCP 连接
    ///
    /// 流程：
    /// 1. 检查熔断器
    /// 2. 构建 Transport
    /// 3. serve_client 建立连接
    /// 4. 存入 service
    pub async fn connect(&self) -> McpResult<McpStatus> {
        // 熔断器检查
        if !self.circuit.allow_request() {
            return Err(McpError::CircuitOpen {
                name: self.name.clone(),
            });
        }

        // 如果已经连接，先断开
        if self.connected.load(Ordering::Acquire) {
            debug!("[MCP:{}] already connected, reconnecting...", self.name);
            self.disconnect_inner().await;
        }

        info!("[MCP:{}] connecting via {}...", self.name, self.config.transport_type());

        // 构建 Transport 并建立连接
        let service = self.build_and_serve().await.map_err(|e| {
            self.circuit.record_failure();
            error!("[MCP:{}] connection failed: {}", self.name, e);
            McpError::Connection {
                name: self.name.clone(),
                source: Box::new(e),
            }
        })?;

        // 存储连接
        {
            let mut guard = self.service.lock().await;
            *guard = Some(service);
        }
        self.connected.store(true, Ordering::Release);
        self.circuit.record_success();

        info!("[MCP:{}] connected successfully", self.name);
        self.events.send(McpEvent::Connected {
            name: self.name.clone(),
        });

        Ok(self.get_status())
    }

    /// 断开 MCP 连接
    pub async fn disconnect(&self) -> McpResult<McpStatus> {
        self.disconnect_inner().await;
        Ok(self.get_status())
    }

    /// 内部断开逻辑
    async fn disconnect_inner(&self) {
        let mut guard = self.service.lock().await;
        if let Some(mut service) = guard.take() {
            drop(guard); // 释放锁后再 close
            debug!("[MCP:{}] closing connection...", self.name);
            if let Err(e) = service.close().await {
                warn!("[MCP:{}] error during close: {:?}", self.name, e);
            }
            info!("[MCP:{}] disconnected", self.name);
        }
        self.connected.store(false, Ordering::Release);
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    /// 获取连接状态
    pub fn get_status(&self) -> McpStatus {
        let health = if self.is_connected() {
            "connected"
        } else if self.circuit.is_open() {
            "failed"
        } else {
            "disconnected"
        };

        McpStatus {
            name: self.name.clone(),
            transport_type: self.config.transport_type().to_string(),
            health: health.to_string(),
            circuit_open: self.circuit.is_open(),
            fail_count: self.circuit.failure_count(),
        }
    }

    /// 调用 MCP Tool
    ///
    /// 失败时自动判断是否为连接错误，若是则触发重连并重试一次。
    pub async fn call_tool(&self, params: CallToolRequestParams) -> McpResult<CallToolResult> {
        self.ensure_connected()?;

        let result = self.call_tool_inner(params.clone()).await;

        match result {
            Ok(r) => Ok(r),
            Err(e) if e.is_connection_error() => {
                warn!(
                    "[MCP:{}] tool call failed with connection error, attempting reconnect...",
                    self.name
                );
                self.disconnect_inner().await;
                // 尝试重连一次
                self.reconnect_once().await?;
                // 重试 tool call
                self.call_tool_inner(params).await
            }
            Err(e) => Err(e),
        }
    }

    /// 获取 MCP Server 的 Tool 列表
    pub async fn list_tools(&self) -> McpResult<Vec<Tool>> {
        self.ensure_connected()?;

        let guard = self.service.lock().await;
        let service = guard
            .as_ref()
            .ok_or_else(|| McpError::NotConnected {
                name: self.name.clone(),
            })?;

        let tools = service.list_all_tools().await.map_err(|e| {
            if self.is_service_connection_error(&e) {
                McpError::ConnectionClosed {
                    name: self.name.clone(),
                    reason: e.to_string(),
                }
            } else {
                McpError::Service(e)
            }
        })?;

        Ok(tools)
    }

    /// 健康检查：检测底层连接是否仍然存活
    pub async fn health_check(&self) -> bool {
        if !self.connected.load(Ordering::Acquire) {
            return false;
        }

        let guard = self.service.lock().await;
        match guard.as_ref() {
            Some(service) => {
                let closed = service.is_closed();
                if closed {
                    warn!("[MCP:{}] health check: connection is closed", self.name);
                }
                !closed
            }
            None => false,
        }
    }

    /// 重置熔断器（用于手动恢复）
    pub fn reset_circuit(&self) {
        self.circuit.reset();
    }

    // ─── 内部方法 ────────────────────────────────────────────

    /// 确保已连接
    fn ensure_connected(&self) -> McpResult<()> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(McpError::NotConnected {
                name: self.name.clone(),
            });
        }
        Ok(())
    }

    /// 执行 tool 调用（不加锁的外部请求通过内部方法中转）
    async fn call_tool_inner(&self, params: CallToolRequestParams) -> McpResult<CallToolResult> {
        let guard = self.service.lock().await;
        let service = guard
            .as_ref()
            .ok_or_else(|| McpError::NotConnected {
                name: self.name.clone(),
            })?;

        // RunningService derefs to Peer<RoleClient>，可直接调用 call_tool
        let result = service.call_tool(params).await.map_err(|e| {
            if self.is_service_connection_error(&e) {
                McpError::ConnectionClosed {
                    name: self.name.clone(),
                    reason: e.to_string(),
                }
            } else {
                McpError::Service(e)
            }
        })?;

        Ok(result)
    }

    /// 执行一次性重连（不触发指数退避循环，由上层控制）
    async fn reconnect_once(&self) -> McpResult<()> {
        // 重置熔断器（单次重连不计入熔断）
        // 注意：持续的 connect 调用会通过 connect() 方法本身的熔断器逻辑来控制
        self.circuit.reset();

        info!("[MCP:{}] attempting single reconnect...", self.name);
        self.events.send(McpEvent::Reconnecting {
            name: self.name.clone(),
            attempt: 1,
        });

        match self.connect().await {
            Ok(_) => {
                self.events.send(McpEvent::Reconnected {
                    name: self.name.clone(),
                });
                Ok(())
            }
            Err(e) => {
                self.events.send(McpEvent::Disconnected {
                    name: self.name.clone(),
                    reason: e.to_string(),
                });
                Err(e)
            }
        }
    }

    /// 带指数退避的重连循环
    pub async fn reconnect_with_backoff(&self) -> McpResult<()> {
        let mut attempt: u32 = 0;
        let mut backoff = self.reconnect_config.initial_backoff;

        loop {
            attempt += 1;

            // 检查最大重试次数
            if let Some(max) = self.reconnect_config.max_retries {
                if attempt > max {
                    let err = format!("max retries ({}) exceeded", max);
                    error!("[MCP:{}] {}", self.name, err);
                    self.events.send(McpEvent::ReconnectFailed {
                        name: self.name.clone(),
                        error: err.clone(),
                    });
                    return Err(McpError::ConnectionClosed {
                        name: self.name.clone(),
                        reason: err,
                    });
                }
            }

            info!(
                "[MCP:{}] reconnecting (attempt {}/{})...",
                self.name,
                attempt,
                self.reconnect_config
                    .max_retries
                    .map_or("∞".to_string(), |m| m.to_string())
            );
            self.events.send(McpEvent::Reconnecting {
                name: self.name.clone(),
                attempt,
            });

            // 断开旧连接
            self.disconnect_inner().await;

            // 尝试连接
            match self.connect().await {
                Ok(_) => {
                    self.events.send(McpEvent::Reconnected {
                        name: self.name.clone(),
                    });
                    return Ok(());
                }
                Err(e) => {
                    warn!("[MCP:{}] reconnect attempt {} failed: {}", self.name, attempt, e);
                    // 指数退避等待
                    tokio::time::sleep(backoff).await;
                    backoff = Duration::from_secs_f64(
                        (backoff.as_secs_f64() * self.reconnect_config.backoff_multiplier)
                            .min(self.reconnect_config.max_backoff.as_secs_f64()),
                    );
                }
            }
        }
    }

    /// 构建 Transport 并建立 serve_client 连接
    async fn build_and_serve(&self) -> McpResult<RunningService<RoleClient, ()>> {
        match &self.config {
            TransportConfig::Stdio { command, args, env } => {
                use rmcp::transport::child_process::TokioChildProcess;

                let mut cmd = tokio::process::Command::new(command);
                cmd.args(args);
                for (k, v) in env {
                    cmd.env(k, v);
                }
                cmd.kill_on_drop(true);

                let transport = TokioChildProcess::new(cmd).map_err(|e| {
                    McpError::Transport(format!("failed to spawn child process: {}", e))
                })?;

                // ().serve(transport) → RunningService<RoleClient, ()>
                let service = ()
                    .serve(transport)
                    .await
                    .map_err(|e| McpError::Transport(format!("serve_client failed: {}", e)))?;

                Ok(service)
            }
            TransportConfig::Http { url } => {
                let transport =
                    rmcp::transport::streamable_http_client::StreamableHttpClientTransport::from_uri(
                        url.clone(),
                    );

                let service = ()
                    .serve(transport)
                    .await
                    .map_err(|e| McpError::Transport(format!("serve_client failed: {}", e)))?;

                Ok(service)
            }
        }
    }

    /// 判断 ServiceError 是否为连接相关的错误
    fn is_service_connection_error(&self, err: &rmcp::service::ServiceError) -> bool {
        let msg = err.to_string().to_lowercase();
        msg.contains("connection")
            || msg.contains("closed")
            || msg.contains("timeout")
            || msg.contains("transport")
            || msg.contains("io error")
            || msg.contains("broken pipe")
    }
}

impl Drop for McpConnection {
    fn drop(&mut self) {
        // 兜底清理：如果连接还活跃，尝试关闭
        if self.connected.load(Ordering::Acquire) {
            debug!("[MCP:{}] McpConnection dropped, cleaning up...", self.name);
            // 无法在 Drop 中执行 async，依赖 RunningService 的 Drop 实现清理
            // RunningService 的 Drop 会取消后台任务并关闭 transport
        }
    }
}
