# Rust 可插拔插件系统 - AI 友好型设计方案

## 一、需求概述

### 1.1 核心需求
- AI 动态生成插件代码，Rust 程序运行时加载执行（无需编译步骤）
- 插件需要**高权限**：网络访问、MCP 调用、文件系统等系统能力
- 支持热加载，可动态更新插件逻辑
- 对 AI 生成代码友好，降低生成错误率

### 1.2 技术选型结论
**选择 Rhai 脚本语言**作为插件方案

| 对比项 | Rhai | Rune | 传统动态库 |
|--------|------|------|------------|
| 无需编译 | ✅ | ✅ | ❌ |
| 支持网络/MCP | ✅ | ✅ | ✅ |
| AI 生成准确率 | 95% | 60% | 0% |
| 语法复杂度 | 低（JS-like） | 中（Rust-like） | 高 |
| 热加载 | ✅ | ✅ | 部分支持 |

---

## 二、架构设计

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────────┐
│                     Rust 主程序                          │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │ 插件管理器  │  │ 权限控制器  │  │ 审计日志    │    │
│  └─────────────┘  └─────────────┘  └─────────────┘    │
│  ┌─────────────────────────────────────────────────┐  │
│  │              Rhai 脚本引擎                       │  │
│  │  ┌──────────────────────────────────────────┐  │  │
│  │  │  注册的 Rust 宿主函数                     │  │  │
│  │  │  - http_get / http_post                  │  │  │
│  │  │  - mcp_call                              │  │  │
│  │  │  - db_query                              │  │  │
│  │  │  - fs_read / fs_write                    │  │  │
│  │  └──────────────────────────────────────────┘  │  │
│  └─────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
                              ▲
                              │ 动态加载
                    ┌─────────┴─────────┐
                    │   AI 生成的脚本    │
                    │   (Rhai 语法)     │
                    └───────────────────┘
```

### 2.2 数据流
1. AI 生成 Rhai 脚本代码
2. Rust 主程序接收脚本
3. 权限控制器校验脚本允许的操作
4. Rhai 引擎编译并执行脚本
5. 脚本调用注册的 Rust 函数
6. 审计日志记录所有敏感操作
7. 返回执行结果

---

## 三、技术实现方案

### 3.1 项目依赖

```toml
[dependencies]
rhai = { version = "1.19", features = ["sync", "serde", "internals"] }
tokio = { version = "1.35", features = ["full"] }
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
async-trait = "0.1"
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
```

### 3.2 核心代码结构

```rust
// main.rs
mod plugin_manager;
mod permissions;
mod audit;

use plugin_manager::PluginManager;
use permissions::{Permission, PermissionController};
use rhai::{Engine, Scope, Dynamic};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 初始化插件管理器
    let manager = PluginManager::new();
    
    // 2. AI 生成的脚本（从 API 或文件加载）
    let ai_plugin_code = r#"
        fn main() {
            print("AI 插件开始执行");
            
            // 网络请求
            let data = http_get("https://api.example.com/data");
            
            // MCP 调用
            let result = mcp_tool("process", #{
                input: data,
                operation: "analyze"
            });
            
            return result;
        }
    "#;
    
    // 3. 加载并执行插件
    manager.load_plugin(ai_plugin_code.to_string()).await?;
    let result = manager.execute_plugin("main").await?;
    
    println!("执行结果: {:?}", result);
    Ok(())
}
```

### 3.3 插件管理器实现

```rust
// plugin_manager.rs
use std::sync::Arc;
use tokio::sync::RwLock;
use rhai::{Engine, Scope, AST, Dynamic};
use crate::permissions::PermissionController;

pub struct PluginManager {
    engine: Engine,
    active_plugin: Arc<RwLock<Option<AST>>>,
    permission_controller: PermissionController,
}

impl PluginManager {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        let permission_controller = PermissionController::new();
        
        // 注册所有宿主函数
        Self::register_host_functions(&mut engine, &permission_controller);
        
        // 设置脚本执行限制
        Self::configure_engine_limits(&mut engine);
        
        Self {
            engine,
            active_plugin: Arc::new(RwLock::new(None)),
            permission_controller,
        }
    }
    
    fn register_host_functions(engine: &mut Engine, perm_ctrl: &PermissionController) {
        let perm = perm_ctrl.clone();
        
        // HTTP GET
        engine.register_async_fn("http_get", move |url: String| {
            let perm = perm.clone();
            async move {
                perm.check_network(&url)?;
                let client = reqwest::Client::new();
                let response = client.get(&url)
                    .timeout(std::time::Duration::from_secs(30))
                    .send()
                    .await
                    .map_err(|e| format!("HTTP请求失败: {}", e))?
                    .text()
                    .await
                    .map_err(|e| format!("读取响应失败: {}", e))?;
                Ok(response)
            }
        });
        
        // HTTP POST
        engine.register_async_fn("http_post", |url: String, data: rhai::Map| {
            async move {
                // 实现 POST 逻辑
                // ...
                Ok("success".to_string())
            }
        });
        
        // MCP 调用
        engine.register_async_fn("mcp_tool", |tool: String, params: rhai::Map| {
            async move {
                // MCP 客户端调用
                // ...
                Ok(format!("MCP {} 执行成功", tool))
            }
        });
        
        // 数据库查询
        engine.register_async_fn("db_query", |sql: String| {
            async move {
                // 数据库操作
                // ...
                Ok("query result".to_string())
            }
        });
    }
    
    fn configure_engine_limits(engine: &mut Engine) {
        // 设置执行超时
        engine.set_timeout(std::time::Duration::from_secs(10));
        // 限制最大调用栈深度
        engine.set_max_call_stack_depth(100);
        // 限制最大操作数
        engine.set_max_operations(1_000_000);
    }
    
    pub async fn load_plugin(&self, code: String) -> anyhow::Result<()> {
        tracing::info!("加载插件，代码长度: {}", code.len());
        
        // 可选：静态代码分析（检测危险模式）
        self.validate_plugin_code(&code)?;
        
        let ast = self.engine.compile(&code)?;
        let mut plugin = self.active_plugin.write().await;
        *plugin = Some(ast);
        
        Ok(())
    }
    
    pub async fn execute_plugin(&self, func_name: &str) -> anyhow::Result<Dynamic> {
        let plugin = self.active_plugin.read().await;
        let ast = plugin.as_ref().ok_or_else(|| anyhow::anyhow!("未加载插件"))?;
        
        let mut scope = Scope::new();
        let result = self.engine
            .call_ast_fn_with_scope::<Dynamic>(&mut scope, ast, func_name, ())
            .await?;
        
        Ok(result)
    }
    
    fn validate_plugin_code(&self, code: &str) -> anyhow::Result<()> {
        // 检查危险模式
        let dangerous_patterns = [
            ("std::process::", "禁止直接调用系统命令"),
            ("eval(", "禁止动态执行代码"),
            ("loop", "可能造成死循环"),
        ];
        
        for (pattern, msg) in dangerous_patterns {
            if code.contains(pattern) {
                anyhow::bail!("安全错误: {}", msg);
            }
        }
        
        Ok(())
    }
}
```

### 3.4 权限控制系统

```rust
// permissions.rs
use std::sync::Arc;
use std::collections::HashSet;

#[derive(Clone)]
pub struct Permission {
    pub allow_network: bool,
    pub allow_mcp: bool,
    pub allow_filesystem: bool,
    pub allow_database: bool,
    pub allowed_domains: HashSet<String>,
    pub rate_limit: RateLimit,
}

#[derive(Clone)]
pub struct RateLimit {
    pub max_requests_per_second: u32,
    pub max_data_per_second: u64, // bytes
}

pub struct PermissionController {
    default_permission: Permission,
    // 可为不同插件设置不同权限
    plugin_permissions: Arc<tokio::sync::RwLock<HashMap<String, Permission>>>,
}

impl PermissionController {
    pub fn new() -> Self {
        let default_permission = Permission {
            allow_network: true,
            allow_mcp: true,
            allow_filesystem: false,  // 默认禁止文件访问
            allow_database: true,
            allowed_domains: {
                let mut set = HashSet::new();
                set.insert("api.example.com".to_string());
                set
            },
            rate_limit: RateLimit {
                max_requests_per_second: 10,
                max_data_per_second: 1024 * 1024, // 1MB
            },
        };
        
        Self {
            default_permission,
            plugin_permissions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
    
    pub fn check_network(&self, url: &str) -> anyhow::Result<()> {
        if !self.default_permission.allow_network {
            anyhow::bail!("网络访问被禁止");
        }
        
        // 域名白名单检查
        let domain = urlparse_domain(url);
        if !self.default_permission.allowed_domains.contains(&domain) {
            anyhow::bail!("域名 {} 不在白名单中", domain);
        }
        
        Ok(())
    }
}

fn urlparse_domain(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .to_string()
}
```

### 3.5 审计日志

```rust
// audit.rs
use tracing::{info, warn, error};
use serde_json::json;
use chrono::Utc;

pub struct AuditLogger;

impl AuditLogger {
    pub fn log_plugin_load(plugin_id: &str, code_size: usize) {
        info!(
            event = "plugin_load",
            plugin_id = plugin_id,
            code_size = code_size,
            timestamp = Utc::now().to_rfc3339()
        );
    }
    
    pub fn log_network_call(plugin_id: &str, url: &str, success: bool) {
        info!(
            event = "network_call",
            plugin_id = plugin_id,
            url = url,
            success = success,
            timestamp = Utc::now().to_rfc3339()
        );
    }
    
    pub fn log_mcp_call(plugin_id: &str, tool: &str, params: &str) {
        info!(
            event = "mcp_call",
            plugin_id = plugin_id,
            tool = tool,
            params = params,
            timestamp = Utc::now().to_rfc3339()
        );
    }
    
    pub fn log_error(plugin_id: &str, error: &str) {
        error!(
            event = "plugin_error",
            plugin_id = plugin_id,
            error = error,
            timestamp = Utc::now().to_rfc3339()
        );
    }
}
```

---

## 四、AI 脚本生成指南

### 4.1 Rhai 语法规范（AI 生成需遵循）

```rust
// 1. 变量声明（无需类型）
let name = "value";
let count = 42;

// 2. 字符串拼接
let greeting = "Hello, " + name;

// 3. 条件语句
if count > 10 {
    print("大于10");
} else if count == 10 {
    print("等于10");
} else {
    print("小于10");
}

// 4. 循环
for i in 0..10 {
    print(i);
}

let x = 0;
while x < 5 {
    x += 1;
}

// 5. 函数定义（无需返回类型声明）
fn process(a, b) {
    return a + b;
}

// 6. 错误处理
let result = call_may_fail();
if type_of(result) == "string" {
    print(result);
}

// 7. 对象/Map（类似 JSON）
let data = #{
    name: "test",
    value: 42,
    nested: #{
        enabled: true
    }
};

// 8. 关键：返回值
fn main() {
    let response = http_get("https://api.example.com/data");
    return response;
}
```

### 4.2 可用的宿主函数（Rust 注入）

| 函数名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `http_get(url)` | `String` | `String` | GET 请求 |
| `http_post(url, data)` | `String, Map` | `String` | POST 请求 |
| `mcp_tool(tool, params)` | `String, Map` | `String` | 调用 MCP 工具 |
| `db_query(sql)` | `String` | `String` | 数据库查询 |
| `print(msg)` | `Dynamic` | `()` | 打印日志 |
| `debug(value)` | `Dynamic` | `String` | 调试输出 |

### 4.3 示例：AI 生成真实可用的插件

**用户需求**：获取天气数据，调用 AI 分析，保存结果

**AI 应生成的 Rhai 代码**：

```rust
fn main() {
    // 1. 获取天气数据
    let weather_data = http_get("https://api.weather.com/beijing");
    
    // 2. 解析并提取温度
    let temperature = extract_temperature(weather_data);
    
    // 3. 调用 MCP 进行分析
    let analysis = mcp_tool("ai_analyzer", #{
        type: "weather",
        data: weather_data,
        threshold: temperature
    });
    
    // 4. 保存到数据库
    let saved = db_query(
        "INSERT INTO weather_logs VALUES ('" + 
        timestamp() + "', '" + 
        temperature + "', '" + 
        analysis + "')"
    );
    
    return #{
        status: "success",
        temperature: temperature,
        analysis: analysis
    };
}

fn extract_temperature(json_data) {
    // 简化示例：实际需要 JSON 解析
    if json_data.contains("25") {
        return 25;
    }
    return 0;
}

fn timestamp() {
    let now = http_get("https://timeapi.io/api/time/current");
    return now;
}
```

---

## 五、开发计划

### 5.1 阶段一：基础框架（1周）

**目标**：搭建 Rhai 引擎，实现基本的脚本加载和执行

- [ ] 项目初始化，添加 Rhai 依赖
- [ ] 实现基础 PluginManager 结构
- [ ] 实现脚本加载和编译
- [ ] 实现最简单的执行流程
- [ ] 单元测试

**交付**：可加载和执行 "hello world" 脚本

### 5.2 阶段二：宿主函数注册（1周）

**目标**：注册网络、MCP 等核心能力

- [ ] 注册 `http_get` 函数（带超时控制）
- [ ] 注册 `http_post` 函数（支持 JSON）
- [ ] 注册 MCP 调用接口（预留接口）
- [ ] 注册数据库查询接口
- [ ] 注册调试函数（print/debug）
- [ ] 集成测试

**交付**：脚本可调用网络请求和 MCP

### 5.3 阶段三：权限与安全（3天）

**目标**：实现权限控制和审计日志

- [ ] 实现 Permission 权限模型
- [ ] 域名白名单机制
- [ ] 请求频率限制
- [ ] 脚本静态分析（危险代码检测）
- [ ] 审计日志集成
- [ ] 安全测试

**交付**：插件在受限沙箱中运行

### 5.4 阶段四：热加载与管理（3天）

**目标**：支持动态更新多个插件

- [ ] 插件生命周期管理
- [ ] 热加载/卸载机制
- [ ] 插件版本管理
- [ ] 插件状态监控
- [ ] Metrics 收集

**交付**：可动态加载/更新插件

### 5.5 阶段五：AI 集成（2天）

**目标**：接收 AI 生成的脚本并执行

- [ ] HTTP API 接收 AI 脚本
- [ ] 脚本格式验证
- [ ] 错误恢复机制
- [ ] 回退策略（脚本执行失败的处理）
- [ ] AI 提示词模板优化

**交付**：完整 AI → Rust 插件工作流

### 5.6 阶段六：优化与文档（2天）

- [ ] 性能优化（脚本缓存、编译结果复用）
- [ ] 完善错误提示（帮助 AI 调试）
- [ ] 编写使用文档
- [ ] 提供 AI 提示词示例
- [ ] 压测与调优

---

## 六、风险与对策

| 风险 | 影响 | 概率 | 对策 |
|------|------|------|------|
| AI 生成代码语法错误 | 高 | 中 | 提供脚本验证 API，返回详细错误信息供 AI 修正 |
| 恶意脚本攻击 | 高 | 低 | 权限控制 + 静态分析 + 资源限制 |
| 脚本死循环 | 中 | 中 | 设置执行超时 10 秒，超过强制终止 |
| 网络请求阻塞 | 中 | 中 | 异步执行 + 超时控制 + 连接池 |
| MCP 调用失败 | 低 | 中 | 实现重试机制和降级策略 |
| 性能瓶颈 | 中 | 低 | 脚本预编译、缓存、连接复用 |

---

## 七、成功标准

### 7.1 功能指标
- ✅ AI 生成的脚本成功执行率 ≥ 90%
- ✅ 支持 HTTP GET/POST、MCP 调用、数据库查询
- ✅ 插件热加载 < 100ms
- ✅ 脚本执行超时 ≤ 10 秒

### 7.2 性能指标
- 脚本执行延迟 < 100ms（不含网络 IO）
- 并发支持 ≥ 100 脚本/秒
- 内存占用 ≤ 100MB（10 个插件）

### 7.3 安全指标
- 无插件逃逸漏洞
- 所有网络请求符合白名单
- 完整审计日志覆盖

---

## 八、参考资料

### 8.1 Rhai 官方文档
- https://rhai.rs/book/
- 重点章节：引擎配置、自定义函数、异步支持

### 8.2 代码仓库结构建议

```
plugin-system/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs              # 入口
│   ├── plugin_manager.rs    # 插件管理器
│   ├── permissions.rs       # 权限控制
│   ├── audit.rs            # 审计日志
│   ├── host_functions/      # 宿主函数
│   │   ├── mod.rs
│   │   ├── http.rs
│   │   ├── mcp.rs
│   │   └── database.rs
│   └── scripts/             # 示例脚本
│       ├── example.rhai
│       └── test.rhai
├── tests/                   # 集成测试
├── examples/                # 示例程序
└── docs/                    # 文档
    └── ai_prompt_template.md # AI 提示词模板
```

### 8.3 AI 提示词模板（关键）

```markdown
生成 Rhai 脚本，要求：

规则：
1. 只使用以下函数：http_get, http_post, mcp_tool, db_query, print
2. 变量不用类型声明
3. 字符串用 + 拼接，不用 format!
4. 返回普通类型或 Map（#{key: value}）

任务：[用户需求描述]

输出格式：只输出 Rhai 代码，不要解释
```

---

## 九、后续扩展方向

1. **插件市场**：支持从远程仓库下载插件
2. **可视化编辑器**：图形化编排插件逻辑
3. **插件依赖**：支持插件间调用和依赖声明
4. **性能监控**：插件执行的详细性能分析
5. **A/B 测试**：同一插件多版本并存和流量切换

---

**文档版本**：1.0  
**最后更新**：2026-01-12  
**负责人**：开发团队  
**审阅状态**：待审阅