//! MCP 单连接生命周期管理
//!
//! ## 模块结构
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                      McpConnection                             │
//! │                    (单连接管理器)                              │
//! │                                                              │
//! │  核心字段:                                                    │
//! │  ├─ service: Mutex<Option<RunningService>>                   │
//! │  ├─ circuit: CircuitBreaker (熔断器)                          │
//! │  ├─ connected: AtomicBool (连接状态标志)                      │
//! │  └─ heartbeat_running: AtomicBool (心跳监控运行标志)          │
//! │                                                              │
//! │  配置:                                                        │
//! │  ├─ config: TransportConfig (传输层配置)                      │
//! │  ├─ reconnect_config: ReconnectConfig (重连策略)               │
//! │  └─ heartbeat_config: HeartbeatConfig (心跳配置)               │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 连接生命周期
//!
//! ```text
//! connect() → 连接成功 → start_heartbeat_monitor() → 后台监控
//!                 ↓                                         ↓
//!            connect()                                检测到断开
//!                 ↓                                         ↓
//!          disconnect_inner()                      auto_reconnect()
//!                 ↓                                         ↓
//!          stop_heartbeat_monitor()                        ↓
//!                 ↓                                    重连成功/失败
//!            service.close()                            ↓
//!                                                       发送事件
//! ```
//!
//! ## 心跳保活机制
//!
//! STDIO 子进程没有内置心跳保活机制，依赖后台监控任务检测连接状态：
//!
//! - **心跳间隔**: 默认 30 秒（可配置）
//! - **存活检测**: 通过 `RunningService.is_closed()` 检测连接状态
//! - **自动重连**: 检测到断开后自动重连（最多 3 次，指数退避）
//! - **状态同步**: 重连结果通过 McpEventBus 推送事件
//!
//! ## 熔断器策略
//!
//! ```text
//! Closed ──(连续失败 3 次)──▶ Open
//! Open   ──(冷却 30s 后)────▶ HalfOpen
//! HalfOpen ──(成功)────────▶ Closed
//! HalfOpen ──(失败)────────▶ Open
//! ```
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use crate::provider::mcp::{McpConnection, TransportConfig, HeartbeatConfig};
//! use std::time::Duration;
//!
//! // 创建连接（默认心跳配置）
//! let conn = McpConnection::new(
//!     "playwright".to_string(),
//!     TransportConfig::Stdio {
//!         command: "npx".to_string(),
//!         args: vec!["-y".to_string(), "@modelcontextprotocol/server-playwright".to_string()],
//!         env: HashMap::new(),
//!     },
//!     http_client,
//!     events,
//! );
//!
//! // 连接并自动启用心跳监控
//! conn.connect().await?;
//!
//! // 调用工具
//! let tools = conn.list_tools().await?;
//! let result = conn.call_tool(params).await?;
//!
//! // 断开连接并停止心跳监控
//! conn.disconnect().await?;
//! ```
//!
//! ## 自定义心跳配置
//!
//! ```rust,ignore
//! use std::time::Duration;
//!
//! let heartbeat_config = HeartbeatConfig {
//!     interval: Duration::from_secs(60),  // 60s 间隔
//!     auto_reconnect: true,              // 启用自动重连
//!     max_auto_reconnect: 5,             // 最多重连 5 次
//! };
//!
//! let conn = McpConnection::new_with_heartbeat(
//!     name, config, http_client, events, heartbeat_config
//! );
//! ```

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
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

/// MCP 服务运行时状态（返回给调用方）
#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    pub name: String,
    pub transport_type: String,
    pub health: String,
    pub circuit_open: bool,
    pub fail_count: u32,
}

/// 心跳监控配置
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// 心跳间隔（STDIO 模式下必须）
    pub interval: Duration,
    /// 检测间隔（两次检测之间的最小间隔，防止频繁检测）
    pub check_interval: Duration,
    /// 是否启用自动重连（检测到断开时）
    pub auto_reconnect: bool,
    /// 最大自动重连次数
    pub max_auto_reconnect: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            check_interval: Duration::from_millis(100),  // 检测间隔 100ms
            auto_reconnect: true,
            max_auto_reconnect: 3,
        }
    }
}

/// MCP 单连接管理器
///
/// 设计要点：
/// - `service: Mutex<Option<RunningService>>` — tokio async Mutex，允许跨 await 持锁
/// - `RunningService` 实现了 `Deref<Target = Peer<RoleClient>>`，可以直接调用 `call_tool` 等方法
/// - `close()` 需要 `&mut self`，通过 Mutex 取出后 drop guard 再关闭
///
/// ## 心跳保活机制
/// - `heartbeat_tx/watch_channel` — 心跳信号通道，用于控制监控任务生命周期
/// - `heartbeat_config` — 心跳间隔和自动重连配置
/// - 后台监控任务在连接后自动启动，断开后自动停止
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
    /// 事件总线（发布状态变更）
    events: SharedEventBus,

    // ─── 心跳保活机制 ──────────────────────────────────────────
    /// 心跳配置（默认 30s 间隔）
    heartbeat_config: HeartbeatConfig,
    /// 心跳运行中标志（AtomicBool，用于控制监控任务生命周期）
    heartbeat_running: Arc<AtomicBool>,
    /// 是否禁用心跳（测试用）
    disable_heartbeat: bool,
    /// 上次活跃时间戳（用于健康检查）
    last_active: AtomicU64,
}

impl McpConnection {
    /// 创建新的连接管理器（初始状态：未连接）
    pub fn new(
        name: String,
        config: TransportConfig,
        events: SharedEventBus,
    ) -> Self {
        Self {
            name,
            config,
            service: Mutex::new(None),
            circuit: CircuitBreaker::new(CircuitBreakerConfig::default()),
            connected: AtomicBool::new(false),
            events,
            heartbeat_config: HeartbeatConfig::default(),
            heartbeat_running: Arc::new(AtomicBool::new(false)),
            disable_heartbeat: false,
            last_active: AtomicU64::new(0),
        }
    }

    /// 创建连接管理器并禁用心跳（用于测试）
    #[allow(dead_code)]
    pub fn new_no_heartbeat(
        name: String,
        config: TransportConfig,
        events: SharedEventBus,
    ) -> Self {
        let mut conn = Self::new(name, config, events);
        conn.disable_heartbeat = true;
        conn
    }

    /// 创建连接管理器（从 Arc 上下文，用于心跳任务获取自身引用）
    #[allow(dead_code)]
    fn from_arc(conn: Arc<McpConnection>) -> Self {
        Self {
            name: conn.name.clone(),
            config: conn.config.clone(),
            service: Mutex::new(None),
            circuit: CircuitBreaker::new(CircuitBreakerConfig::default()),
            connected: AtomicBool::new(false),
            events: conn.events.clone(),
            heartbeat_config: conn.heartbeat_config.clone(),
            heartbeat_running: Arc::clone(&conn.heartbeat_running),
            disable_heartbeat: false,
            last_active: AtomicU64::new(0),
        }
    }

    /// 建立 MCP 连接
    ///
    /// 流程：
    /// 1. 检查熔断器
    /// 2. 构建 Transport
    /// 3. serve_client 建立连接
    /// 4. 启动心跳监控任务
    /// 5. 存入 service
    pub async fn connect(&self) -> McpResult<McpStatus> {
        // 熔断器检查
        if !self.circuit.allow_request() {
            return Err(McpError::CircuitOpen {
                name: self.name.clone(),
            });
        }

        // 如果已经连接，先断开（会停止旧的心跳监控）
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
        self.update_last_active();

        // 启动心跳监控（非测试模式）
        if !self.disable_heartbeat {
            // 从 Arc<Self> 获取自身引用（安全：self 必然来自某个 Arc）
            let this: Arc<McpConnection> = unsafe {
                let ptr = std::ptr::from_ref(self);
                Arc::increment_strong_count(ptr);
                Arc::from_raw(ptr)
            };
            let name = self.name.clone();
            let config = self.heartbeat_config.clone();
            let heartbeat_running = Arc::clone(&self.heartbeat_running);

            // 设置运行标志
            heartbeat_running.store(true, Ordering::Release);

            tokio::spawn(async move {
                Self::run_heartbeat_loop(name, config, heartbeat_running, this).await;
            });
        } else {
            debug!("[MCP:{}] heartbeat disabled (test mode)", self.name);
        }

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
        // 停止心跳监控
        self.stop_heartbeat_monitor();

        let mut guard = self.service.lock().await;
        if let Some(mut service) = guard.take() {
            drop(guard); // 释放锁后再 close
            debug!("[MCP:{}] closing connection... (5s timeout)", self.name);
            
            // 优化：给 close 添加超时，避免卡住
            match tokio::time::timeout(Duration::from_secs(5), service.close()).await {
                Ok(Ok(reason)) => {
                    info!("[MCP:{}] disconnected gracefully: {:?}", self.name, reason);
                }
                Ok(Err(e)) => {
                    warn!("[MCP:{}] error during close: {:?}", self.name, e);
                }
                Err(_) => {
                    // 超时，直接 drop（不等待子进程响应）
                    warn!("[MCP:{}] close timed out, forcing drop", self.name);
                }
            }
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

    /// 重置熔断器（用于手动恢复）
    pub fn reset_circuit(&self) {
        self.circuit.reset();
    }

    // ─── 心跳保活机制 ──────────────────────────────────────────

    /// 更新最后活跃时间戳
    fn update_last_active(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_active.store(now, Ordering::Release);
    }

    /// 获取最后活跃时间戳
    #[allow(dead_code)]
    fn get_last_active(&self) -> u64 {
        self.last_active.load(Ordering::Acquire)
    }

    /// 心跳监控主循环（独立函数，避免 Arc Self:: 方法调用歧义）
    async fn run_heartbeat_loop(
        name: String,
        config: HeartbeatConfig,
        heartbeat_running: Arc<AtomicBool>,
        this: Arc<McpConnection>,
    ) {
        let mut consecutive_failures = 0u32;

        debug!(
            "[MCP:{}] starting heartbeat monitor (interval={:?}, auto_reconnect={})",
            name, config.interval, config.auto_reconnect
        );

        loop {
            // 检查运行标志
            if !heartbeat_running.load(Ordering::Acquire) {
                debug!("[MCP:{}] heartbeat monitor stopped by flag", name);
                break;
            }

            // 定时心跳检测
            if this.connected.load(Ordering::Acquire) {
                // 检测连接是否真的存活
                let is_alive = this.detect_connection_alive().await;

                if !is_alive {
                    consecutive_failures += 1;
                    warn!(
                        "[MCP:{}] heartbeat detected connection died ({}/{}), auto_reconnect={}",
                        name, consecutive_failures, config.max_auto_reconnect, config.auto_reconnect
                    );

                    if config.auto_reconnect {
                        let success = this.heartbeat_auto_reconnect().await;
                        if !success {
                            // 重连失败，停止监控
                            debug!("[MCP:{}] heartbeat auto-reconnect failed, stopping monitor", name);
                            break;
                        } else {
                            consecutive_failures = 0;
                        }
                    } else {
                        // 不自动重连，发送断开事件
                        this.events.send(McpEvent::Disconnected {
                            name: name.clone(),
                            reason: "heartbeat detected connection died".to_string(),
                        });
                        break;
                    }
                } else {
                    consecutive_failures = 0;
                }
            }

            // 等待下一个心跳间隔
            tokio::time::sleep(config.interval).await;
        }

        // 清理运行标志
        heartbeat_running.store(false, Ordering::Release);
        debug!("[MCP:{}] heartbeat monitor exited", name);
    }

    /// 停止心跳监控任务（立即生效）
    fn stop_heartbeat_monitor(&self) {
        // 设置停止标志，后台任务下次检查时会立即退出
        self.heartbeat_running.store(false, Ordering::Release);
        debug!("[MCP:{}] heartbeat monitor stop flag set", self.name);
    }

    /// 检测连接是否真的存活（通过检查 RunningService 状态）
    async fn detect_connection_alive(&self) -> bool {
        let guard = self.service.lock().await;
        match guard.as_ref() {
            Some(service) => {
                // 检查服务是否已关闭
                !service.is_closed()
            }
            None => false,
        }
    }

    /// 执行自动重连（心跳监控专用）
    async fn heartbeat_auto_reconnect(&self) -> bool {
        if !self.heartbeat_config.auto_reconnect {
            return false;
        }

        let max_retries = self.heartbeat_config.max_auto_reconnect;
        let name = self.name.clone();

        for attempt in 1..=max_retries {
            info!(
                "[MCP:{}] heartbeat auto-reconnect attempt {}/{}",
                name, attempt, max_retries
            );

            // 先断开旧连接（不触发停止监控）
            {
                let mut guard = self.service.lock().await;
                if let Some(mut service) = guard.take() {
                    let _ = service.close().await;
                }
            }
            self.connected.store(false, Ordering::Release);

            // 尝试重新连接
            match self.build_and_serve().await {
                Ok(service) => {
                    // 存储新连接
                    {
                        let mut guard = self.service.lock().await;
                        *guard = Some(service);
                    }
                    self.connected.store(true, Ordering::Release);
                    self.circuit.record_success();
                    self.update_last_active();

                    info!("[MCP:{}] heartbeat auto-reconnect succeeded", name);
                    self.events.send(McpEvent::Reconnected {
                        name: name.clone(),
                    });
                    return true;
                }
                Err(e) => {
                    warn!(
                        "[MCP:{}] heartbeat auto-reconnect attempt {} failed: {}",
                        name, attempt, e
                    );
                    self.circuit.record_failure();

                    // 指数退避
                    let backoff = Duration::from_secs(2_u64.pow(attempt as u32).min(60));
                    tokio::time::sleep(backoff).await;
                }
            }
        }

        // 所有重连尝试都失败
        error!("[MCP:{}] heartbeat auto-reconnect exhausted (max {})", name, max_retries);
        self.events.send(McpEvent::ReconnectFailed {
            name: name.clone(),
            error: "max auto-reconnect attempts exceeded".to_string(),
        });
        false
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

    // ─── Windows PATH 辅助函数 ──────────────────────────────────────────

    /// 构建 Windows 上需要优先注入到 PATH 的关键目录列表
    fn build_windows_key_paths() -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        // 1. %APPDATA%\npm（npm 全局包路径，如 npx.cmd 所在）
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let npm_path = std::path::Path::new(&appdata).join("npm");
            if npm_path.exists() {
                paths.push(npm_path);
            }
        }

        // 2. nodejs 安装目录（npx.exe/npm.exe 所在）
        let nodejs_candidates: Vec<std::path::PathBuf> =
            if let Ok(pf) = std::env::var("ProgramFiles") {
                vec![
                    std::path::Path::new(&pf).join("nodejs"),
                    std::path::PathBuf::from("D:\\Program Files\\nodejs"),
                    std::path::PathBuf::from("C:\\Program Files\\nodejs"),
                ]
            } else {
                vec![
                    std::path::PathBuf::from("D:\\Program Files\\nodejs"),
                    std::path::PathBuf::from("C:\\Program Files\\nodejs"),
                ]
            };
        for p in &nodejs_candidates {
            if p.exists() {
                paths.push(p.clone());
                break; // 找到一个就足够
            }
        }

        paths
    }

    /// 在 Windows 上解析命令的实际可执行路径
    ///
    /// Windows 命令（如 `npx`）通常是 `.cmd` 批处理文件，
    /// CreateProcessW 无法直接执行，必须通过 `cmd /c` 调用。
    ///
    /// 返回 `(original_resolved, cmd_wrapper_exe, args_for_cmd)`：
    /// - `original_resolved`：命令在 PATH 中解析后的完整路径（如果需要）
    /// - `cmd_wrapper_exe`：Some(exe) 表示需要通过 `cmd /c exe args` 执行
    /// - `cmd_wrapper_exe`：None 表示是普通 exe，可直接执行
    fn resolve_windows_command(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> (Option<std::path::PathBuf>, Option<String>, Vec<String>) {
        // 如果命令包含路径分隔符或扩展名，直接返回
        let cmd_path = std::path::Path::new(command);
        if cmd_path.parent().map_or(false, |p| !p.as_os_str().is_empty())
            || cmd_path.extension().is_some()
        {
            return (None, None, args.to_vec());
        }

        // 获取当前进程可见的 PATH（已注入 key_paths 后的版本）
        let current_path = std::env::var_os("PATH").unwrap_or_default();

        // 从用户 env 覆盖（如果用户指定了 PATH）
        let search_path = env
            .get("PATH")
            .map(std::env::split_paths)
            .map(Iterator::collect::<Vec<_>>)
            .unwrap_or_else(|| std::env::split_paths(&current_path).collect());

        // 查找命令对应的文件（尝试多种扩展名）
        for dir in &search_path {
            for ext in &["", ".cmd", ".bat", ".exe", ".com"] {
                let candidate = dir.join(format!("{}{}", command, ext));
                if candidate.exists() {
                    let ext_lower = ext.to_lowercase();
                    // .cmd/.bat 批处理文件必须通过 cmd /c 执行
                    if ext_lower == ".cmd" || ext_lower == ".bat" {
                        let exe_name = candidate.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(command)
                            .to_string();
                        return (Some(candidate), Some(exe_name), args.to_vec());
                    }
                    // .exe/.com 可以直接执行，但仍然用 cmd /c 保持一致
                    return (Some(candidate), None, args.to_vec());
                }
            }
        }

        // 未找到匹配文件，返回原始值
        (None, None, args.to_vec())
    }

    /// 构建 Transport 并建立 serve_client 连接
    async fn build_and_serve(&self) -> McpResult<RunningService<RoleClient, ()>> {
        match &self.config {
            TransportConfig::Stdio { command, args, env } => {
                use rmcp::transport::child_process::TokioChildProcess;

                // ── 构建 tokio::Command ──────────────────────────────────
                let cmd = if cfg!(target_os = "windows") {
                    let mut full_cmd = tokio::process::Command::new("cmd");
                    full_cmd.arg("/c");

                    // 1. 设置 PATH（注入 npm/nodejs 路径）
                    let key_paths = Self::build_windows_key_paths();
                    if !key_paths.is_empty() {
                        if let Some(current_path) = std::env::var_os("PATH") {
                            let mut paths = key_paths;
                            for p in std::env::split_paths(&current_path) {
                                if !paths.contains(&p) {
                                    paths.push(p);
                                }
                            }
                            let full_path =
                                std::env::join_paths(&paths).unwrap_or_else(|_| current_path.clone());
                            debug!(
                                "[MCP:{}] PATH will be set to: {}",
                                self.name,
                                full_path.to_string_lossy()
                            );
                            full_cmd.env("PATH", &full_path);
                        }
                    }

                    // 2. 查找命令实际路径（resolve .cmd/.bat/.exe）
                    let resolved = Self::resolve_windows_command(command, &args, env);
                    debug!(
                        "[MCP:{}] resolved command: resolved={:?} cmd_exe={:?}",
                        self.name,
                        resolved.0,
                        resolved.1
                    );

                    if resolved.1.is_some() {
                        // 批处理文件（.cmd/.bat）→ 通过 cmd /c 执行
                        // args: ["/c", "npx", "-y", "@modelcontextprotocol/server-playwright"]
                        full_cmd.arg(&resolved.1.unwrap());
                        for a in &resolved.2 {
                            full_cmd.arg(a);
                        }
                    } else {
                        // 普通命令，直接执行
                        full_cmd.arg(command);
                        full_cmd.args(args);
                    }

                    // 3. 注入用户自定义 env
                    for (k, v) in env {
                        full_cmd.env(k, v);
                    }

                    full_cmd.kill_on_drop(true);
                    full_cmd
                } else {
                    // 非 Windows 平台：直接执行
                    let mut full_cmd = tokio::process::Command::new(command);
                    full_cmd.args(args);
                    for (k, v) in env {
                        full_cmd.env(k, v);
                    }
                    full_cmd.kill_on_drop(true);
                    full_cmd
                };

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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    /// 测试：验证 %APPDATA%\npm 路径是否可访问
    #[test]
    #[cfg(target_os = "windows")]
    fn test_npm_path_exists() {
        let appdata = std::env::var("APPDATA").expect("APPDATA not set");
        let npm_path = Path::new(&appdata).join("npm");
        assert!(
            npm_path.exists(),
            "npm path does not exist: {}",
            npm_path.display()
        );
        println!("npm_path exists: {}", npm_path.display());
    }

    /// 测试：验证 npx 可执行文件是否存在（多个可能路径）
    #[test]
    #[cfg(target_os = "windows")]
    fn test_npx_exe_exists() {
        // 可能存放 npx 的目录
        let search_paths: Vec<_> = vec![
            Path::new(&std::env::var("APPDATA").unwrap()).join("npm"),
            std::path::PathBuf::from("D:\\Program Files\\nodejs"),
            std::path::PathBuf::from("C:\\Program Files\\nodejs"),
        ];

        let mut found = false;
        for base in &search_paths {
            if !base.exists() {
                continue;
            }
            for ext in &["cmd", "exe", "bat"] {
                let npx_path = base.join("npx").with_extension(*ext);
                if npx_path.exists() {
                    println!("npx found: {}", npx_path.display());
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        assert!(found, "npx not found in any of: {:?}", search_paths);
    }

    /// 测试：打印当前 PATH 中的所有路径
    #[test]
    #[cfg(target_os = "windows")]
    fn test_print_current_path() {
        let path = std::env::var("PATH").expect("PATH not set");
        let paths: Vec<_> = std::env::split_paths(&path).collect();
        println!("\n=== Current PATH ({} entries) ===", paths.len());
        for (i, p) in paths.iter().enumerate() {
            let exists = p.exists();
            println!("{:2}. [{}] {}", i + 1, if exists { "OK" } else { "MISS" }, p.display());
        }
        println!("=================================\n");

        // 验证 %APPDATA%\npm 是否在 PATH 中
        let appdata = std::env::var("APPDATA").expect("APPDATA not set");
        let npm_path = Path::new(&appdata).join("npm");
        let npm_in_path = paths.iter().any(|p| p == &npm_path);
        println!(
            "%APPDATA%\\npm in PATH: {} ({})\n",
            if npm_in_path { "YES" } else { "NO" },
            npm_path.display()
        );
    }

    /// 测试：验证 PATH 去重后路径数量是否减少
    #[test]
    #[cfg(target_os = "windows")]
    fn test_path_deduplication() {
        let path = std::env::var("PATH").expect("PATH not set");
        let paths: Vec<_> = std::env::split_paths(&path).collect();
        let unique: HashSet<_> = paths.iter().collect();
        println!(
            "Original paths: {}, Unique: {} (duplicates: {})",
            paths.len(),
            unique.len(),
            paths.len() - unique.len()
        );
        assert_eq!(paths.len(), unique.len(), "PATH should not have duplicates");
    }

    /// 测试：验证 npm path 会被追加到 PATH 前面
    #[test]
    #[cfg(target_os = "windows")]
    fn test_npm_path_prepended() {
        let paths = super::McpConnection::build_windows_key_paths();
        assert!(
            !paths.is_empty(),
            "build_windows_key_paths should return at least one path"
        );
        let first = &paths[0];
        println!(
            "First key_path: {}",
            first.display()
        );
        assert!(
            first.exists(),
            "key path should exist: {}",
            first.display()
        );
    }

    /// 测试：resolve_windows_command 能找到 npx.cmd
    #[test]
    #[cfg(target_os = "windows")]
    fn test_resolve_windows_command_finds_npx() {
        use std::collections::HashMap;
        let env = HashMap::new();
        let (resolved, cmd_exe, args) =
            super::McpConnection::resolve_windows_command("npx", &["-y".to_string(), "some-package".to_string()], &env);
        println!("resolved={:?}, cmd_exe={:?}, args={:?}", resolved, cmd_exe, args);
        // cmd_exe 应该是 Some("npx.cmd")
        assert!(
            cmd_exe.is_some(),
            "npx should be resolved to a cmd wrapper"
        );
        println!("npx resolved to cmd wrapper: {}", cmd_exe.unwrap());
    }
}
