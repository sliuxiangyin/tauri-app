# MCP 服务管理模块

Pure Rust 实现的 MCP（Model Context Protocol）服务管理器，支持多服务器并发管理和事件驱动。

## 核心约束

- **禁止在 mcp 模块中直接引入 `tauri::AppHandle`**
- 事件通知通过 `mpsc::Sender` 异步转发，外部消费并转发给 Tauri
- 所有数据库操作必须在 mcp 模块外部进行

## 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                          Tauri 层                                │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                  事件转发任务                             │    │
│  │  event_receiver ──→ app_handle.emit()                   │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              ↑                                   │
│                     McpEventSender                              │
└─────────────────────────────┼───────────────────────────────────┘
                              │
┌─────────────────────────────┼───────────────────────────────────┐
│                    MCP 模块（pure Rust）                          │
│                              ↓                                   │
│  ┌────────────┐    ┌────────────┐    ┌────────────────────┐    │
│  │ McpState   │───→│ McpQueue   │───→│ ServerManager      │    │
│  │ (单例)     │    │ (并发控制) │    │ (连接+工具缓存)    │    │
│  └────────────┘    └────────────┘    └────────────────────┘    │
│        │                                     │                  │
│        ↓                                     ↓                  │
│  ┌────────────┐                      ┌────────────────────┐    │
│  │ event.rs   │                      │ McpConnection      │    │
│  │ McpEvent   │                      │ (rmcp RunningService)│   │
│  │ McpEventSender│                   └────────────────────┘    │
│  └────────────┘                                                    │
└─────────────────────────────────────────────────────────────────┘
```

## 目录结构

```
src-tauri/src/provider/mcp/
├── mod.rs           # 模块入口，导出公共类型
├── config.rs        # McpServerConfig、TransportConfig 配置类型
├── error.rs         # McpError 错误类型
├── event.rs         # McpEvent 事件类型和 McpEventSender
├── connection.rs    # McpConnection 单连接管理
├── process.rs       # StdioProcessManager STDIO 进程管理
├── manager.rs       # ServerManager 服务器管理和工具缓存
├── queue.rs         # McpQueue + QueueProcessor 并发控制队列
└── state.rs         # McpState 全局状态单例
```

## 核心类型

### McpState

全局状态单例，所有 MCP 操作都通过它进行。

```rust
pub struct McpState {
    event_sender: Arc<RwLock<Option<McpEventSender>>>,
    queue: Arc<McpQueue>,
    manager: Arc<ServerManager>,
}
```

### McpServerConfig

MCP 服务器配置，**不涉及数据库操作**。

```rust
pub struct McpServerConfig {
    pub id: String,           // 唯一标识
    pub name: String,          // 显示名称
    pub description: Option<String>,
    pub transport: TransportConfig,
}

pub enum TransportConfig {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
}
```

### McpEvent

事件类型，用于通知外部 MCP 服务器状态变化。

```rust
pub enum McpEvent {
    ServerPending { server_id, name },
    ServerInstalling { server_id, name, progress },
    ServerConnecting { server_id, name },
    ServerConnected { server_id, name, tool_count },
    ServerFailed { server_id, name, error },
    ServerDisconnected { server_id, name, reason },
    ServerStopped { server_id, name },
    ServerRemoved { server_id, name },
}
```

### ServerState

服务器状态枚举。

```rust
pub enum ServerState {
    Pending,           // 等待队列处理
    Installing,        // 正在安装
    Connecting,        // 正在连接
    Connected,         // 已连接
    Disconnected { reason: String },
    Failed { error: String },
    Stopped,           // 已停止
}
```

## 功能列表

| 功能 | 方法 | 说明 |
|------|------|------|
| 初始化 | `init(configs)` | 外部传入已保存的服务器配置 |
| 启动 | `start()` | 启动队列处理器（后台任务） |
| 创建服务器 | `create_server(config)` | 入队列异步处理 |
| 更新服务器 | `update_server(id, config)` | 先停止旧服务，再创建新的 |
| 删除服务器 | `remove_server(id)` | 直接停止并删除 |
| 刷新服务器 | `refresh_server(id)` | 手动重新连接 |
| 调用工具 | `call_tool(server_id, tool_name, arguments)` | 失败自动触发重连 |
| 列表工具 | `list_tools()` | 获取所有服务器的工具 |
| 列表配置 | `list_configs()` | 快速返回（无连接检测） |
| 列表服务器 | `list_servers()` | 获取所有服务器状态 |
| 检查连接 | `is_connected(server_id)` | 检查服务器是否已连接 |
| 关闭 | `shutdown()` | 优雅关闭所有连接 |

## 连接断开检测方案

采用**被动检测 + 主动刷新**方案：

1. **调用工具时检测**：调用 `call_tool` 时如果连接失败，标记状态为 `Disconnected` 并触发自动重连
2. **手动刷新**：`refresh_server(server_id)` 可手动触发重新连接
3. **不采用后台心跳**：避免多个 MCP 服务时增加系统负担

```rust
// 调用工具时检测连接
pub async fn call_tool(&self, server_id: &str, tool_name: &str, arguments: Value) -> Result<Value> {
    match self.manager.call_tool(server_id, tool_name, arguments).await {
        Ok(result) => Ok(result),
        Err(e) if is_connection_error(&e) => {
            // 连接断开，标记状态并触发重连
            self.mark_disconnected(server_id).await;
            self.queue.enqueue(QueueItem::Reconnect { id: server_id.to_string() });
            return Err(e);
        }
        Err(e) => Err(e),
    }
}
```

## 并发控制

使用信号量（Semaphore）控制最大并发数，默认值为 3。

```rust
pub struct McpQueue {
    sender: mpsc::Sender<QueueItem>,
    max_concurrency: usize,
    semaphore: Arc<Semaphore>,
    processing: Arc<RwLock<HashMap<String, VecDeque<QueueItem>>>>,
}
```

## 使用示例

### 1. 初始化（简化版）

只需传入 `max_concurrency` 和 `configs`：

```rust
use crate::provider::mcp::{McpState, McpEvent};

// 创建 MCP 状态（最大并发数 3，配置列表）
let configs = vec![
    McpServerConfig::new_stdio("server1", "MCP Server 1", "npx", vec!["-y", "@mobilenext/mobile-mcp".to_string()]),
];
let mcp_state = McpState::new(3, configs);

// 获取事件接收器（在 Tauri 层启动事件消费任务）
let mut event_receiver = mcp_state.get_event_receiver();
let app_handle = app_handle.clone();
tokio::spawn(async move {
    while let Some(event) = event_receiver.recv().await {
        match event {
            McpEvent::ServerConnected { server_id, name, tool_count } => {
                app_handle.emit("mcp:server-connected", serde_json::json!({
                    "server_id": server_id, "name": name, "tool_count": tool_count
                })).ok();
            }
            McpEvent::ServerFailed { server_id, name, error } => {
                app_handle.emit("mcp:server-failed", serde_json::json!({
                    "server_id": server_id, "name": name, "error": error
                })).ok();
            }
            _ => {}
        }
    }
});

// 获取状态
let servers = mcp_state.list_servers().await;
```

### 2. 创建服务器

```rust
let config = McpServerConfig::new_stdio(
    "my-mcp-server",
    "My MCP Server",
    "npx",
    vec!["-y", "@some/mcp-package".to_string()],
);

mcp_state.create_server(config).await?;
```

### 3. 调用工具

```rust
let arguments = serde_json::json!({
    "query": "hello world"
});

let result = mcp_state.call_tool(
    "my-mcp-server",
    "search",
    arguments
).await?;
```

### 4. 刷新服务器（手动重连）

```rust
mcp_state.refresh_server("my-mcp-server").await?;
```

### 5. 关闭

```rust
mcp_state.shutdown().await;
```

## Tauri 命令示例

```rust
#[tauri::command]
pub async fn create_mcp_server(
    state: State<'_, Arc<McpState>>,
    config: McpServerConfig,
) -> Result<(), String> {
    state.create_server(config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_mcp_tools(state: State<'_, Arc<McpState>>) -> Result<Vec<ToolWithSource>, String> {
    Ok(state.list_tools().await)
}

#[tauri::command]
pub async fn call_mcp_tool(
    state: State<'_, Arc<McpState>>,
    server_id: String,
    tool_name: String,
    arguments: Value,
) -> Result<Value, String> {
    state.call_tool(&server_id, &tool_name, arguments)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_mcp_server(
    state: State<'_, Arc<McpState>>,
    server_id: String,
) -> Result<(), String> {
    state.refresh_server(&server_id).await.map_err(|e| e.to_string())
}
```

## 队列操作类型

```rust
pub enum QueueItem {
    Create { id: String, config: McpServerConfig },
    Update { id: String, config: McpServerConfig },  // 先停止再创建
    Reconnect { id: String },                         // 重新连接
    Stop { id: String },                              // 停止
    Remove { id: String },                            // 删除
}
```

## 依赖

- `rmcp` - MCP 协议实现
- `tokio` - 异步运行时
- `serde` - 序列化
- `tracing` - 日志