# MCP 模块文档

> MCP (Model Context Protocol) 纯运行时连接管理器

## 目录

- [模块结构](#模块结构)
- [核心组件](#核心组件)
- [心跳保活机制](#心跳保活机制)
- [熔断器策略](#熔断器策略)
- [事件系统](#事件系统)
- [使用示例](#使用示例)
- [文件清单](#文件清单)

---

## 模块结构

```text
┌─────────────────────────────────────────────────────────────┐
│                        McpManager                            │
│                    (连接池管理器)                            │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ connections: RwLock<HashMap<String, Arc<McpConnection>>>│
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  事件总线: McpEventBus (broadcast channel)                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      McpConnection                           │
│                    (单连接生命周期)                          │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ service: Mutex<Option<RunningService<RoleClient, ()>>>│
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  熔断器: CircuitBreaker                                     │
│  心跳监控: 后台任务 (heartbeat_running)                      │
└─────────────────────────────────────────────────────────────┘
```

---

## 核心组件

### McpManager

MCP 运行时管理器，负责管理多个 MCP 服务器的活跃连接。

**设计要点：**
- `connections: RwLock<HashMap<>>` — 读多写少，标准 RwLock 足够
- 每个 McpConnection 内部使用 `tokio::sync::Mutex` 管理 RunningService
- 通过 `Arc` 共享 McpConnection，允许并发工具调用

**主要方法：**

| 方法 | 说明 |
|------|------|
| `connect(name, config)` | 建立 MCP 连接 |
| `disconnect(name)` | 断开 MCP 连接 |
| `restart(name, config)` | 重启 MCP 连接 |
| `call_tool(name, params)` | 调用 MCP 工具 |
| `list_tools(name)` | 获取工具列表 |
| `get_status(name)` | 获取连接状态 |
| `subscribe_events()` | 订阅状态变更事件 |

### McpConnection

单连接管理器，管理一个 MCP 服务器的完整生命周期。

**核心字段：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `service` | `Mutex<Option<RunningService>>` | 活跃的 MCP 连接 |
| `circuit` | `CircuitBreaker` | 熔断器 |
| `connected` | `AtomicBool` | 连接状态标志 |
| `heartbeat_running` | `Arc<AtomicBool>` | 心跳监控运行标志 |
| `config` | `TransportConfig` | 传输层配置 |

### TransportConfig

传输层配置，支持两种模式：

```rust
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
```

---

## 心跳保活机制

### 问题背景

STDIO 子进程没有内置心跳保活机制，依赖后台监控任务检测连接状态。

### 工作流程

```text
connect() → 连接成功 → start_heartbeat_monitor() → 后台监控
                 ↓                                         ↓
            connect()                                检测到断开
                 ↓                                         ↓
          disconnect_inner()                      auto_reconnect()
                 ↓                                         ↓
          stop_heartbeat_monitor()                        ↓
                 ↓                                    重连成功/失败
            service.close()                            ↓
                                                       发送事件
```

### 配置参数

```rust
pub struct HeartbeatConfig {
    /// 心跳间隔（默认 30s）
    pub interval: Duration,
    /// 是否启用自动重连
    pub auto_reconnect: bool,
    /// 最大自动重连次数（默认 3）
    pub max_auto_reconnect: u32,
}
```

### 检测与重连

- **存活检测**: 通过 `RunningService.is_closed()` 检测连接状态
- **自动重连**: 检测到断开后自动重连（指数退避）
- **状态同步**: 重连结果通过 McpEventBus 推送事件

---

## 熔断器策略

### 状态机

```text
Closed ──(连续失败 3 次)──▶ Open
Open   ──(冷却 30s 后)────▶ HalfOpen
HalfOpen ──(成功)────────▶ Closed
HalfOpen ──(失败)────────▶ Open
```

### 配置

```rust
pub struct CircuitBreakerConfig {
    /// 连续失败多少次后打开熔断器（默认 3）
    pub failure_threshold: u32,
    /// 熔断器打开后的冷却时间（默认 30s）
    pub cooldown: Duration,
}
```

---

## 事件系统

### McpEvent 事件类型

```rust
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
```

### 订阅事件

```rust,ignore
let mut rx = mcp_manager.subscribe_events();

tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        match event {
            McpEvent::Reconnected { name } => {
                println!("[{}] reconnected!", name);
            }
            McpEvent::ReconnectFailed { name, error } => {
                println!("[{}] reconnect failed: {}", name, error);
            }
            _ => {}
        }
    }
});
```

---

## 使用示例

### 基本连接

```rust,ignore
use crate::provider::mcp::{McpManager, TransportConfig};
use std::collections::HashMap;

// 创建 MCP 管理器
let mcp = Arc::new(McpManager::new(http_client));

// 连接 MCP 服务
mcp.connect("playwright", TransportConfig::Stdio {
    command: "npx".to_string(),
    args: vec![
        "-y".to_string(),
        "@modelcontextprotocol/server-playwright".to_string()
    ],
    env: HashMap::new(),
}).await?;

// 调用工具
let result = mcp.call_tool("playwright", params).await?;

// 获取状态
let status = mcp.get_status("playwright");
```

### 自定义心跳配置

```rust,ignore
use crate::provider::mcp::{McpConnection, HeartbeatConfig};
use std::time::Duration;

let heartbeat_config = HeartbeatConfig {
    interval: Duration::from_secs(60),  // 60s 间隔
    auto_reconnect: true,              // 启用自动重连
    max_auto_reconnect: 5,             // 最多重连 5 次
};

let conn = McpConnection::new_with_heartbeat(
    name, config, http_client, events, heartbeat_config
);
```

### STDIO 配置示例

```rust,ignore
// Windows npx 配置
TransportConfig::Stdio {
    command: "npx".to_string(),
    args: vec![
        "-y".to_string(),
        "@modelcontextprotocol/server-playwright".to_string()
    ],
    env: HashMap::new(),
}

// HTTP 配置
TransportConfig::Http {
    url: "http://localhost:3000/mcp".to_string(),
}
```

---

## 设计原则

1. **纯被动式连接**: 所有连接由前端显式触发，不支持启动时自动连接
2. **单例模式**: McpManager 保持 Arc 单例模式，不手动实现 Clone
3. **异步优先**: 使用 tokio async Mutex 管理连接，支持跨 await 持锁
4. **事件驱动**: 状态变更通过 McpEventBus 广播

---

## 文件清单

| 文件 | 说明 |
|------|------|
| `mod.rs` | 模块入口，McpManager 定义 |
| `connection.rs` | McpConnection 单连接管理 |
| `circuit.rs` | 熔断器实现 |
| `event.rs` | 事件系统 |
| `error.rs` | 错误类型定义 |

---

## 相关文档

- [Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [MCP 协议规范](https://modelcontextprotocol.io)