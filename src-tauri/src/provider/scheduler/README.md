以下是更新后的完整 `README.md` 文档，涵盖了调度器的所有功能：

```markdown
# Lightweight Periodic Task Scheduler

一个基于 `tokio` 的轻量级周期性任务调度器，支持多种调度策略、任务管理和优雅关闭。

## 特性

- ✅ **多种调度策略**：支持固定延迟（FixedDelay）和固定速率（FixedRate）两种模式
- ✅ **命名任务管理**：按名称添加、移除单个任务
- ✅ **并发控制**：支持限制任务的最大并发实例数
- ✅ **优雅关闭**：等待所有正在执行的任务完成后才退出
- ✅ **任务隔离**：支持创建多个独立的调度器实例
- ✅ **协作式取消**：使用原子标志进行安全的任务取消
- ✅ **灵活的闭包**：支持异步任务，可执行 HTTP 请求、数据库操作等

## 快速开始

```rust
use std::time::Duration;
use scheduler::Scheduler;

#[tokio::main]
async fn main() {
    let scheduler = Scheduler::new();
    
    // 添加一个每 3 秒执行一次的固定速率任务
    scheduler.add_fixed_rate("tick", Duration::from_secs(3), || async {
        println!("tick");
    }).unwrap();
    
    // 运行 10 秒
    tokio::time::sleep(Duration::from_secs(10)).await;
    
    // 优雅关闭
    scheduler.shutdown().await;
}
```

## 调度策略

### 1. 固定延迟（FixedDelay）

任务执行完成后等待固定时间再开始下一次执行。

```rust
scheduler.add_fixed_delay("task", Duration::from_secs(3), || async {
    // 任务执行 5 秒
    tokio::time::sleep(Duration::from_secs(5)).await;
});
// 实际间隔：5秒执行 + 3秒等待 = 8秒
```

**执行时序**：
```
开始(0s) -> 执行(5s) -> 等待(3s) -> 开始(8s)
```

### 2. 固定速率（FixedRate）

严格按照固定周期开始执行，**不等待任务完成**。如果任务执行时间超过周期，允许多个实例并发执行。

```rust
scheduler.add_fixed_rate("task", Duration::from_secs(3), || async {
    // 任务执行 5 秒
    tokio::time::sleep(Duration::from_secs(5)).await;
});
// 实际间隔：严格 3 秒周期，任务会重叠
```

**执行时序**：
```
开始(0s) -> 执行(5s)
开始(3s) -> 执行(5s)
开始(6s) -> 执行(5s)
开始(9s) -> 执行(5s)
```

## 核心功能

### 创建调度器

```rust
// 创建独立调度器（非单例）
let scheduler = Scheduler::new();

// 或使用 Default trait
let scheduler = Scheduler::default();
```

可以创建多个独立的调度器实例：

```rust
let scheduler1 = Scheduler::new();  // 用于 HTTP 任务
let scheduler2 = Scheduler::new();  // 用于数据库任务
```

### 添加周期性任务

```rust
// 固定延迟模式（默认）
scheduler.add_periodic("task1", Duration::from_secs(5), || async {
    // 任务逻辑
})?;

// 固定延迟模式（显式）
scheduler.add_fixed_delay("task2", Duration::from_secs(5), || async {
    // 任务逻辑
})?;

// 固定速率模式
scheduler.add_fixed_rate("task3", Duration::from_secs(5), || async {
    // 任务逻辑
})?;

// 指定调度策略
use scheduler::SchedulingPolicy;
scheduler.add_periodic_with_policy("task4", Duration::from_secs(5), SchedulingPolicy::FixedRate, || async {
    // 任务逻辑
})?;
```

### 并发控制

限制固定速率任务的最大并发实例数：

```rust
// 最多允许 3 个实例同时运行
scheduler.add_fixed_rate_with_limit(
    "limited_task",
    Duration::from_secs(1),   // 每 1 秒触发一次
    3,                         // 最大并发数
    || async {
        // 假设任务执行 2 秒
        tokio::time::sleep(Duration::from_secs(2)).await;
    },
)?;
```

### 任务管理

```rust
// 移除单个任务
scheduler.remove("task_name")?;

// 停止所有任务（不等待）
scheduler.stop();

// 优雅关闭（等待所有任务完成）
scheduler.shutdown().await;

// 查询任务数量
let count = scheduler.task_count();

// 检查是否已停止
let stopped = scheduler.is_stopped();

// 获取任务的活跃实例数（仅 FixedRate 模式）
if let Some(active) = scheduler.get_active_count("task_name") {
    println!("当前活跃实例数: {}", active);
}
```

## 实际应用示例

### 1. HTTP 健康检查

```rust
use std::sync::Arc;
use reqwest::Client;

let scheduler = Scheduler::new();
let client = Client::new();
let urls = Arc::new(vec![
    "https://api1.example.com/health",
    "https://api2.example.com/health",
]);

scheduler.add_fixed_rate("health_check", Duration::from_secs(30), move || {
    let urls = urls.clone();
    let client = client.clone();
    async move {
        for url in urls.iter() {
            match client.get(url).timeout(Duration::from_secs(5)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("✅ {} is healthy", url);
                }
                _ => {
                    println!("❌ {} is down", url);
                }
            }
        }
    }
})?;
```

### 2. 批量数据同步（带并发限制）

```rust
let scheduler = Scheduler::new();

scheduler.add_fixed_rate_with_limit(
    "data_sync",
    Duration::from_secs(60),  // 每分钟同步一次
    5,                         // 最多 5 个并发同步任务
    || async {
        let records = fetch_pending_records().await;
        let mut handles = vec![];
        
        for record in records {
            handles.push(tokio::spawn(async move {
                sync_to_remote(record).await;
            }));
        }
        
        futures::future::join_all(handles).await;
    },
)?;
```

### 3. 多调度器隔离

```rust
// 高频实时任务调度器
let realtime_scheduler = Scheduler::new();
realtime_scheduler.add_fixed_rate("sensor_read", Duration::from_millis(100), || async {
    read_sensors().await;
})?;

// 低频后台任务调度器
let background_scheduler = Scheduler::new();
background_scheduler.add_fixed_delay("cleanup", Duration::from_secs(3600), || async {
    clean_temp_files().await;
})?;

// 独立控制生命周期
realtime_scheduler.stop();  // 只停止实时任务
```

### 4. 动态调整周期的任务

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

let counter = Arc::new(AtomicUsize::new(0));
let scheduler = Scheduler::new();

scheduler.add_fixed_rate("adaptive", Duration::from_secs(1), {
    let counter = counter.clone();
    async move {
        let current = counter.fetch_add(1, Ordering::Relaxed);
        
        if current < 10 {
            // 前 10 次频繁执行
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            // 之后降低频率
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
})?;
```

## API 参考

### Scheduler 方法

| 方法 | 说明 | 返回值 |
|------|------|--------|
| `new()` | 创建新的调度器实例 | `Self` |
| `add_periodic()` | 添加周期性任务（默认固定延迟） | `Result<(), SchedulerError>` |
| `add_fixed_delay()` | 添加固定延迟任务 | `Result<(), SchedulerError>` |
| `add_fixed_rate()` | 添加固定速率任务 | `Result<(), SchedulerError>` |
| `add_fixed_rate_with_limit()` | 添加带并发限制的固定速率任务 | `Result<(), SchedulerError>` |
| `add_periodic_with_policy()` | 添加任务并指定策略 | `Result<(), SchedulerError>` |
| `remove()` | 移除指定任务 | `Result<(), SchedulerError>` |
| `stop()` | 停止所有任务（不等待） | `()` |
| `shutdown()` | 优雅关闭（等待任务完成） | `Future<Output = ()>` |
| `task_count()` | 获取任务数量 | `usize` |
| `is_stopped()` | 检查是否已停止 | `bool` |
| `get_active_count()` | 获取任务活跃实例数 | `Option<usize>` |

### 调度策略

```rust
pub enum SchedulingPolicy {
    FixedDelay,  // 固定延迟：完成后等待
    FixedRate,   // 固定速率：周期开始
}
```

### 错误类型

```rust
pub enum SchedulerError {
    TaskAlreadyExists(String),  // 任务名已存在
    TaskNotFound(String),       // 任务不存在
    LockError(String),          // 锁错误
}
```

## 设计说明

### 核心架构

```
Scheduler
├── tasks: Mutex<HashMap<String, TaskHandle>>
├── stopped: Arc<AtomicBool>
└── TaskHandle
    ├── cancel: Arc<AtomicBool>
    ├── join: JoinHandle<()>
    └── active_count: Arc<AtomicUsize>
```

### 协作式取消

- 每个任务周期性检查 `cancel` 和 `stopped` 标志
- 当标志为 `true` 时，任务在下一个周期退出
- 不强制中断正在执行的业务逻辑，保证安全性

### 线程安全

- 所有方法接受 `&self`，调度器可被多线程共享（`Send + Sync`）
- 使用 `tokio::sync::Mutex` 保护任务注册表
- 任务闭包要求 `Fn() -> Fut + Send + Sync + 'static`

## 注意事项

1. **固定速率任务可能堆积**：如果任务耗时超过周期且无并发限制，会创建大量并发实例
2. **使用并发限制**：对于耗时未知的任务，建议使用 `add_fixed_rate_with_limit`
3. **优雅关闭**：`shutdown()` 会等待所有任务完成，确保在 `main` 函数结束前调用
4. **任务名称唯一性**：同一调度器内，任务名称必须唯一

## 性能建议

| 场景 | 推荐调度模式 | 并发限制 |
|------|-------------|----------|
| 快速任务（<10ms） | FixedRate | 不需要 |
| 中速任务（10ms-1s） | FixedRate | 建议设置 |
| 慢速任务（>1s） | FixedDelay | 不需要 |
| HTTP 请求 | FixedRate + Limit | 5-10 |
| 数据库操作 | FixedDelay | 不需要 |
| 实时监控 | FixedRate | 根据负载 |

## 完整示例

运行示例：

```bash
cargo run --example basic
cargo run --example http_batch
cargo run --example concurrent_limit
```

## 许可证

MIT 或 Apache-2.0

## 依赖

- `tokio` 1.35+ (运行时)
- `thiserror` 1.0 (错误处理)
- `chrono` 0.4 (示例用)
- `reqwest` 0.11 (HTTP 示例)
- `futures` 0.3 (并发工具)
```

这份文档涵盖了：
- ✅ 两种调度策略的详细说明
- ✅ 并发控制的实现方法
- ✅ 多调度器实例的使用场景
- ✅ 完整的 API 参考
- ✅ 实际应用示例
- ✅ 性能建议和注意事项