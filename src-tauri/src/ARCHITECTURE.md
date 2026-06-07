# 项目目录结构与架构规范

## 一、目录结构

```
src-tauri/src/
├── commands/          # Tauri 命令层（前端入口）
│   ├── mod.rs
│   ├── chat.rs
│   ├── llm.rs
│   ├── mcp.rs
│   └── ...
│
├── services/         # 服务层（面向对象 + 依赖注入）
│   ├── mod.rs                    # Trait 定义
│   ├── traits.rs                 # DbAccessor、McpClient trait
│   ├── db/           # 数据库 CRUD
│   │   ├── mod.rs
│   │   └── ...
│   ├── llm/          # LLM 相关服务
│   │   ├── mod.rs
│   │   └── tool_executor.rs
│   ├── llm_service.rs    # LlmService 结构体
│   ├── mcp_service.rs    # McpService 结构体
│   ├── chat_model_service.rs  # ChatModelService 结构体
│   ├── chat_tools_service.rs  # ChatToolsService 结构体
│   └── ...
│
├── provider/         # 协议/运行时层（可独立发布）
│   ├── mod.rs
│   ├── llm/          # LLM Provider
│   │   ├── providers/
│   │   ├── agent/
│   │   ├── types.rs
│   │   └── ...
│   ├── mcp/          # MCP 运行时（实现 McpClient Trait）
│   ├── cache/        # 缓存
│   ├── scheduler/
│   └── wechat/
│
├── entity/           # SeaORM 实体定义
│   ├── mod.rs
│   └── ...
│
├── migration/        # 数据库迁移
│
├── db/              # 数据库连接管理（实现 DbAccessor Trait）
│   ├── mod.rs
│   └── connection.rs
│
├── types/           # 跨模块共享类型
│
└── lib.rs           # 应用入口（依赖注入组装）
```

## 二、各层职责与约束

### 1. commands 层（方案 B：薄包装）

**职责**：
- 接收前端 Tauri 调用
- 参数校验
- 调用注入的 Service 结构体方法
- 错误转换为 `String`

**约束**：
```text
✅ 允许：
- 通过 tauri::State 获取 Service 实例
- 调用 Service 方法
- 参数校验

❌ 禁止：
- 直接操作数据库
- 直接调用 provider 实现
- 包含业务逻辑
- 手动获取 DbState 再调用 services
```

**示例**：
```rust
#[tauri::command]
pub async fn get_all_mcps(
    mcp_service: State<'_, Arc<McpService>>,
) -> Result<Vec<McpServiceDto>, String> {
    mcp_service.get_all().await
}
```

### 2. services 层（方案 B：面向对象 + 依赖注入）

**职责**：
- 业务逻辑编排
- 持有 Trait 对象或具体依赖
- 调用 services::db 执行数据持久化

**约束**：
```text
✅ 允许：
- 持有 Arc<dyn DbAccessor>
- 持有 Arc<Cache>
- 持有 Arc<McpManager>
- 通过构造器注入依赖

❌ 禁止：
- 导入 provider 下具体实现（如 OpenAiProvider）
- 包含框架特定代码（tauri::State）
```

**示例**：
```rust
// McpService 结构体
pub struct McpService {
    db: Arc<dyn DbAccessor>,
    mcp: Arc<McpManager>,
}

impl McpService {
    pub fn new(db: Arc<dyn DbAccessor>, mcp: Arc<McpManager>) -> Self {
        Self { db, mcp }
    }

    pub async fn get_all(&self) -> Result<Vec<McpServiceDto>, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        // ...业务逻辑
    }
}
```

### 3. services/db 层（实现层）

**职责**：
- 数据库 CRUD 操作
- 实体与 DTO 转换
- 复杂查询封装

**约束**：
```text
✅ 允许：
- 导入 entity 模块
- 使用 SeaORM
- 依赖 db 模块

❌ 禁止：
- 包含业务逻辑
- 导入 commands、services（不含 db）模块
```

### 4. provider 层（Trait 实现层）

**职责**：
- 定义统一接口 Trait
- 实现具体 Provider
- 协议封装

**约束**（最严格解耦层）：
```text
✅ 允许：
- 定义 Trait（如 LlmProvider、McpClient）
- 实现 Trait（如 OpenAiProvider、OllamaProvider）
- 内部模块间引用

❌ 禁止：
- 导入 services、commands、db、entity
- 持有业务状态
```

### 5. db 层（连接管理层）

**职责**：
- 实现 `DbAccessor` Trait
- 数据库连接管理
- 连接池

**约束**：
```text
✅ 允许：
- 懒加载连接
- 连接复用

❌ 禁止：
- 业务逻辑
- 表操作（由 services::db 负责）
```

## 三、Trait 接口定义（核心）

### 在 services/traits.rs 中定义

```rust
//! Trait 接口定义 - 依赖注入核心

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

pub use crate::db::DbError;

/// 数据库访问接口
#[async_trait]
pub trait DbAccessor: Send + Sync {
    async fn get(&self) -> Result<Arc<DatabaseConnection>, DbError>;
}

/// MCP 客户端接口
#[async_trait]
pub trait McpClient: Send + Sync {
    async fn call_tool(&self, name: &str, params: Value) -> Result<Value, McpError>;
    async fn get_tools(&self, name: &str) -> Result<Vec<Tool>, McpError>;
    fn get_status(&self, name: &str) -> Option<McpStatus>;
    fn list_all_status(&self) -> Vec<McpStatus>;
    fn get_tools_count(&self) -> usize;
}
```

### Trait 实现者映射

| Trait | 实现者 |
|-------|--------|
| `DbAccessor` | `db::DbState` |
| `McpClient` | `provider::mcp::McpManager` |

## 四、依赖注入组装（lib.rs）

```rust
// lib.rs - setup 闭包中

// 1. 创建基础组件
let db_state = db::DbState::new(app_handle.clone());
app.manage(db_state.clone());

let cache = Cache::open("./app-cache").expect("...");
let cache_arc = Cache::set_global(cache).expect("...");
app.manage(cache_arc.clone());

let mcp_manager = Arc::new(provider::mcp::McpManager::new());
app.manage(mcp_manager.clone());

// 2. 创建 McpService 并注入依赖
let db_accessor: Arc<dyn services::traits::DbAccessor> = Arc::new(db_state.clone());
let mcp_service = Arc::new(services::mcp_service::McpService::new(
    db_accessor,
    Arc::clone(&mcp_manager),
));
app.manage(mcp_service.clone());

// 3. 其他 Service 类似方式注入...
```

## 五、方案对比

### 方案 A：无状态函数（传统方式）

```rust
// 问题：
// 1. Commands 层需要手动获取 DbState
// 2. 每个函数签名都要传递依赖
// 3. 难以发现隐藏依赖
pub async fn get_all_mcps(
    db_state: &DbState,
    mcp_manager: &McpManager,
) -> Result<Vec<McpServiceDto>, String> {
    let db = db_state.get().await?;
    // ...
}
```

**Commands 层**：
```rust
#[tauri::command]
pub async fn get_all_mcps(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
) -> Result<Vec<McpServiceDto>, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::get_all_mcps(&db, &mcp_manager).await
}
```

### 方案 B：面向对象 + 依赖注入（推荐）

```rust
// 优势：
// 1. Commands 层极简，直接调用 Service 方法
// 2. 依赖在构造时注入，隐藏依赖一目了然
// 3. 可测试性强（可注入 Mock）
// 4. 可扩展性好（新增依赖只需修改构造器）
pub struct McpService {
    db: Arc<dyn DbAccessor>,
    mcp: Arc<McpManager>,
}
```

**Commands 层**：
```rust
#[tauri::command]
pub async fn get_all_mcps(
    mcp_service: State<'_, Arc<McpService>>,
) -> Result<Vec<McpServiceDto>, String> {
    mcp_service.get_all().await
}
```

## 六、依赖方向图（方案 B）

```
┌─────────────────────────────────────────────────────────────┐
│                         Frontend                             │
└─────────────────────────┬───────────────────────────────────┘
                          │ invoke
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                      commands 层                            │
│  - 获取 State<Arc<Service>>                               │
│  - 参数校验                                                │
│  - 调用 service.method()                                   │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                      services 层                            │
│  - 持有构造时注入的依赖                                     │
│  - Service 结构体封装业务逻辑                              │
└───────┬─────────────────────────────────────┬───────────────┘
        │                                    │
        ▼                                    ▼
┌───────────────────────┐    ┌───────────────────────────────┐
│  Arc<dyn DbAccessor>   │    │      Arc<McpManager>         │
└───────────────────────┘    └───────────────────────────────┘
        │                                    │
        │  实现                              │ 实现
        ▼                                    ▼
┌───────────────────────┐    ┌───────────────────────────────┐
│      db 层            │    │     provider/mcp/             │
│   (DbState)          │    │   (McpManager → McpClient)    │
└───────────────────────┘    └───────────────────────────────┘
```

## 七、命名规范

### 模块命名
- `commands/` - 命令层（Tauri 入口）
- `services/` - 服务层（面向对象 + Trait 依赖）
- `services/db/` - 数据持久化
- `provider/` - 协议层（定义 Trait，可独立发布）
- `db/` - 数据库连接（实现 DbAccessor）
- `entity/` - SeaORM 实体

### Trait 命名
- `{Feature}Client` - 客户端（如 `McpClient`）
- `{Feature}Accessor` - 访问器（如 `DbAccessor`）

## 八、错误处理约定

```rust
// 1. commands 层：转换为 String
Err(e) => Err(e.to_string())

// 2. services 层：Result<T, String>
pub async fn service_method() -> Result<T, String>

// 3. services::db 层：使用具体错误
pub async fn db_method() -> Result<T, DbError>

// 4. provider 层：使用 provider 内部错误类型
pub async fn provider_method() -> Result<T, ProviderError>
```

## 九、新增模块检查清单

新增模块时，回答以下问题：

| 问题 | 答案示例 |
|------|----------|
| 谁调用它？ | commands 或其他 services |
| 它调用谁？ | services::db 或 provider |
| 它需要 DB 吗？ | 在构造器中接收 `Arc<dyn DbAccessor>` |
| 它需要外部服务吗？ | 在构造器中接收对应依赖 |

**如果依赖具体实现而非 Trait，说明分层位置不正确。**

## 十、适用场景

| 新增 | 应放在 | 示例 |
|------|--------|------|
| Tauri 命令 | commands/ | `commands/mcp.rs` |
| 业务逻辑 | services/ | `services/mcp_service.rs` |
| 数据库操作 | services::db/ | `services/db/mcp.rs` |
| 外部协议封装 | provider/ | `provider/mcp/mod.rs` |
| 连接管理 | db/ | `db/mod.rs` |
| 表结构 | entity/ | `entity/mcp.rs` |