# MCP (Model Context Protocol) 服务模块

这个模块提供了一个完整的 MCP 服务集成，支持连接外部 MCP 服务并获取工具列表。

## 功能特性

- 🔌 **支持多种传输方式**
  - Stdio - 本地二进制进程
  - HTTP - 远程 HTTP 服务

- 🔀 **多服务管理** - 同时支持连接多个 MCP 服务

- ⚡ **工具缓存** - 工具列表采用 5 分钟 TTL 缓存，支持手动刷新

- 🔄 **自动重试** - 连接失败自动重试 3 次，然后返回错误

- 🛠️ **完整的工具调用** - 支持执行 MCP 工具并获取结果

## 架构

```
┌─────────────────────────────────────────────────────────┐
│                    McpManager (管理器)                   │
│  - 管理多个连接                                          │
│  - 路由请求到对应的客户端                               │
└─────────────────┬───────────────────────────────────────┘
                  │
        ┌─────────┼─────────┐
        │         │         │
     ┌──▼──┐  ┌──▼──┐   ┌──▼──┐
     │Client│  │Client│   │Client│
     │  #1  │  │  #2  │   │  #N  │
     └──┬──┘  └──┬──┘   └──┬──┘
        │         │         │
    ┌───▼──┐  ┌──▼───┐  ┌──▼────┐
    │Stdio │  │HTTP  │  │ Stdio │
    └──────┘  └──────┘  └───────┘
```

## API 文档

### 连接服务

```typescript
// Tauri Frontend 调用示例
await invoke('mcp_connect', {
  req: {
    service_id: 'my-service',
    name: 'My MCP Service',
    config: {
      transport: 'stdio',
      command: '/usr/bin/mcp-server',
      args: ['--port', '5000'],
      env: {}
    }
  }
});
```

### 获取工具列表

```typescript
// 获取缓存的工具列表
const tools = await invoke('mcp_list_tools', {
  req: {
    service_id: 'my-service',
    force_refresh: false  // 使用缓存
  }
});

// 强制刷新工具列表
const freshTools = await invoke('mcp_list_tools', {
  req: {
    service_id: 'my-service',
    force_refresh: true   // 忽略缓存，重新获取
  }
});
```

### 调用工具

```typescript
const result = await invoke('mcp_call_tool', {
  req: {
    service_id: 'my-service',
    tool_name: 'get_weather',
    arguments: {
      location: 'Beijing',
      units: 'celsius'
    }
  }
});

// 结果结构：
// {
//   content: [
//     { type: 'text', text: 'Weather info...' }
//   ],
//   is_error: false
// }
```

### 列出所有连接的服务

```typescript
const services = await invoke('mcp_list_services');

// 返回：
// [
//   {
//     service_id: 'my-service',
//     name: 'My MCP Service',
//     config: { ... },
//     connected: true,
//     last_connected_at: 1715600000
//   }
// ]
```

### 检查连接状态

```typescript
const isConnected = await invoke('mcp_is_service_connected', {
  service_id: 'my-service'
});
```

### 断开连接

```typescript
await invoke('mcp_disconnect', {
  service_id: 'my-service'
});
```

### 清除工具缓存

```typescript
await invoke('mcp_clear_tools_cache', {
  service_id: 'my-service'
});
```

## 模块结构

```
src/provider/mcp/
├── mod.rs              # 模块导出
├── error.rs            # 错误类型定义 (McpError)
├── types.rs            # 数据结构定义
├── transport.rs        # 传输层抽象 (Stdio/HTTP)
├── client.rs           # MCP 客户端 (单个连接的处理)
├── manager.rs          # MCP 管理器 (多个连接的管理)
└── README.md           # 本文件
```

## 错误处理

所有错误都会被转换为 `String` 类型返回给前端：

```rust
pub enum McpError {
    ConnectionError(String),        // 连接错误
    ProtocolError(String),          // 协议错误
    CommunicationError(String),     // 通信错误
    ToolNotFound(String),           // 工具不存在
    ToolExecutionError(String),     // 工具执行失败
    ServiceNotConnected(String),    // 服务未连接
    ConnectionFailedAfterRetries,   // 重试 3 次后连接失败
    ServiceNotFound(String),        // 服务不存在
    // ... 其他错误类型
}
```

## 缓存策略

- **工具列表缓存**：5 分钟 TTL
- **过期方式**：基于时间戳比较
- **强制刷新**：可通过 `force_refresh: true` 参数绕过缓存

## 重试机制

- **重试次数**：3 次
- **重试延迟**：每次 500 毫秒
- **失败处理**：3 次都失败后返回 `ConnectionFailedAfterRetries` 错误

## 使用场景示例

### 场景 1：连接本地 MCP 服务

```typescript
// 连接到本地 stdio 服务
await invoke('mcp_connect', {
  req: {
    service_id: 'local-tools',
    name: 'Local Tools Server',
    config: {
      transport: 'stdio',
      command: '/opt/mcp-servers/tools-server',
      args: [],
      env: {}
    }
  }
});

// 列出工具
const tools = await invoke('mcp_list_tools', {
  req: {
    service_id: 'local-tools',
    force_refresh: false
  }
});
```

### 场景 2：连接远程 HTTP MCP 服务

```typescript
// 连接到 HTTP 服务
await invoke('mcp_connect', {
  req: {
    service_id: 'remote-api',
    name: 'Remote API Server',
    config: {
      transport: 'http',
      url: 'http://api.example.com/mcp'
    }
  }
});
```

### 场景 3：并行调用多个服务

```typescript
// 连接多个服务
await Promise.all([
  invoke('mcp_connect', { req: { service_id: 'service1', ... } }),
  invoke('mcp_connect', { req: { service_id: 'service2', ... } }),
  invoke('mcp_connect', { req: { service_id: 'service3', ... } })
]);

// 列出所有服务
const allServices = await invoke('mcp_list_services');
console.log(`Connected: ${allServices.length} services`);
```

## 依赖

- `rmcp` - MCP 协议库
- `tokio` - 异步运行时
- `serde/serde_json` - 序列化/反序列化
- `async-trait` - 异步 trait 支持
- `thiserror` - 错误处理

## 限制和注意事项

1. **Stdio 模式**：每个请求都会启动一个新进程，性能较差，适合轻量级服务
2. **HTTP 模式**：需要服务商支持 JSON-RPC 2.0 协议
3. **缓存时间**：固定 5 分钟，不可配置（可后续改进）
4. **并发连接**：使用 `Arc<RwLock>` 支持线程安全的并发访问

## 未来改进

- [ ] 支持 SSE (Server-Sent Events) 传输
- [ ] 使其工程支持缓存时间配置
- [ ] 添加重试策略配置
- [ ] 支持连接池
- [ ] 添加性能指标收集
- [ ] 支持事件流订阅

