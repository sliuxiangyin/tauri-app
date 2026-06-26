# Pipeline Executor 数据流与文件说明

> 本文档说明 [`pipeline_executor`](../planner/pipeline_executor/) 模块的整体设计、数据流转路径以及每个文件的职责。
> 适合作为该模块的入门导读和修改参考。

---

## 一、模块定位

`pipeline_executor` 是 Universal Agent 三层规划架构的**驱动入口**。它把以下三个独立 Agent 串联成一个可执行的 Pipeline：

```text
TaskPlannerAgent  →  TaskPlan
                         │
                         ▼
                    StageScheduler （DAG 调度）
                         │
                         ▼ per ready stage
              ExecutionPlannerAgent  →  ExecutionPlan
                                              │
                                              ▼ per step in topological order
                                       ReactAgent.run_step()
```

**职责边界**：

| 模块 | 关心什么 | 不关心什么 |
|---|---|---|
| TaskPlannerAgent | What（任务阶段 + 领域） | 怎么执行、用什么工具 |
| ExecutionPlannerAgent | How（在领域规则下怎么执行） | 工具参数、调用顺序 |
| ReactAgent | Do（工具选择 + 参数 + 调用） | 上层规划、DAG 调度 |
| **pipeline_executor** | **When（执行顺序）、Retry（失败重规划）** | **具体规划内容、工具实现** |

---

## 二、整体数据流图

```text
外部调用方（service / test）
  │
  │  ① 构造 TaskExecutionRequest
  │  ② executor.subscribe() → broadcast::Receiver
  │  ③ executor.run(request) → JoinHandle
  ▼
TaskPipelineExecutor::run
  │
  │  broadcast::Sender ──┐
  │                      │
  │  ┌───────────────────┘
  │  │
  │  ▼  emit TaskStarted
  │  TaskPlannerAgent.run()  ──→  TaskPlan
  │  │                      ▲
  │  │  LlmProvider.send_message()
  │  │  （LlmAgentBase 持有 provider）
  │  │
  │  ▼  emit TaskPlanned
  │  StageScheduler::new(task_plan)
  │  │
  │  └─ loop (DAG 拓扑序)
  │       │
  │       ▼  ready_stages() ── 多个并行起点
  │       │
  │       └─ for each ready stage:
  │              │
  │              ▼
  │       StageRuntimeContext::build(stage, previous_outputs, ...)
  │              │
  │              ▼
  │       ExecutionPlannerAgent.run()  ──→  ExecutionPlan
  │              │
  │              ▼  emit ExecutionPlanned
  │       StageScheduler::topological_steps(plan)
  │              │
  │              ▼ per step in topo order
  │       ReactAgent.run_step(step, &StepContext)
  │              │
  │              ├─ Ok(step_result) → emit StepFinished(success)
  │              └─ Err(e)         → emit StepFinished(failed)
  │                                  │
  │                                  ▼
  │                          decide_stage_replan()
  │                                  │
  │                                  ├─ ReplanStage  → loop 本 stage 重试
  │                                  └─ GiveUp       → emit StageFailed
  │                                                     │
  │                                                     ▼
  │                                            decide_task_replan()
  │                                                     │
  │                                                     ├─ ReplanTask   → replan_task()
  │                                                     │                  │
  │                                                     │                  ▼
  │                                                     │           TaskPlannerAgent 再跑一次
  │                                                     │                  │
  │                                                     │                  ▼
  │                                                     │           current_plan = new_plan
  │                                                     │           continue 'task_loop
  │                                                     └─ GiveUp      → break
  │
  │  ▼  emit TaskFinished (Ok) / TaskFailed (Err)
  │  return TaskExecutionResult
  ▼
JoinHandle → Result<TaskExecutionResult>
```

---

## 三、文件清单

| 文件 | 行数级别 | 职责 |
|---|---|---|
| `mod.rs` | 短 | 模块入口，公开 re-export 主要类型 |
| `types.rs` | 中 | 全部数据结构：Request / Result / Record / 错误 |
| `event.rs` | 短 | 进度事件枚举 `TaskExecutionEvent` |
| `context.rs` | 中 | Stage 运行时上下文 + InputSource 解析 |
| `scheduler.rs` | 中 | DAG 调度器 + Step 拓扑排序 |
| `replan.rs` | 短 | 重规划策略 + 决策函数 |
| `executor.rs` | 长 | `TaskPipelineExecutor` 主体 |

### 3.1 `mod.rs` — 模块入口

**职责**：声明子模块、对外 re-export 常用类型，避免外部调用方写深层路径。

**公开符号**：
- `TaskPipelineExecutor`（executor.rs）
- `TaskExecutionEvent`（event.rs）
- `TaskExecutionRequest` / `TaskExecutionResult` / `StageExecutionRecord` / `StepExecutionRecord`（types.rs）
- `TaskStatus` / `StageStatus` / `StepStatus`（types.rs）
- `PipelineError`（types.rs）
- `ReplanPolicy` / `ReplanDecision` / `decide_stage_replan` / `decide_task_replan`（replan.rs）
- `StageScheduler`（scheduler.rs）
- `StageRuntimeContext`（context.rs）

### 3.2 `types.rs` — 数据结构

**职责**：定义 Pipeline Executor 全部输入/输出/记录类型，与 Tauri 解耦。

**关键类型**：

| 类型 | 用途 |
|---|---|
| `TaskExecutionRequest` | service 层构造的输入，含 request_id、user_request、available_domains/tools、replan 限制等 |
| `TaskExecutionResult` | 执行最终结果，含 task_plan、stage_records、overall_status、final_outputs |
| `TaskStatus` | 整体状态：`Completed` / `PartialSuccess` / `Failed` |
| `StageExecutionRecord` | 单个 Stage 的完整轨迹（status / replan_count / execution_plans / step_records / outputs / duration） |
| `StageStatus` | Stage 状态：`Pending` / `Running` / `Succeeded` / `Failed` / `Replanned` |
| `StepExecutionRecord` | 单个 Step 的执行记录（tool_calls / output / error） |
| `StepStatus` | Step 状态 |
| `PipelineError` | Pipeline 层错误（含 `LlmError` 透传变体） |

**设计要点**：
- 所有公开类型都实现 `Clone`（`broadcast::Sender` 发送事件时需要）
- 所有公开类型都实现 `Serialize`（service 层 emit 到 Tauri 时直接序列化）
- `PipelineError` 用 `thiserror::Error` 实现 `From<LlmError>`，调用方用 `?` 一键转换

### 3.3 `event.rs` — 进度事件

**职责**：定义执行过程中的全部进度事件，**与 Tauri 解耦**。

**关键类型**：
- `TaskExecutionEvent` 枚举，11 个变体（`#[serde(tag = "kind")]`）：

| 变体 | 何时发送 |
|---|---|
| `TaskStarted` | `executor.run()` 进入 `execute()` 时 |
| `TaskPlanned` | Task Planner 成功返回 TaskPlan 后 |
| `StageStarted` | 每个 stage attempt 刚开始时（含 replan 重试） |
| `StageReplanning` | Stage 决定重规划时 |
| `ExecutionPlanned` | Execution Planner 成功返回 ExecutionPlan 后 |
| `StepStarted` | 每个 step 开始时 |
| `StepFinished` | 每个 step 结束时（成功或失败都发） |
| `StageFinished` | Stage 成功完成时 |
| `StageFailed` | Stage 重试耗尽时 |
| `TaskFinished` | 任务整体成功 |
| `TaskFailed` | 任务整体失败（含部分结果） |

**设计要点**：
- 用 `#[serde(tag = "kind", rename_all = "snake_case")]`：service 层 emit 时直接 `app.emit(channel, &event)` 即可
- 不包含 `tauri::*` 依赖，可被任何调用方使用

### 3.4 `context.rs` — 运行时上下文

**职责**：构造 Stage 执行所需的运行时上下文，**核心是 `InputSource` 解析**。

**关键类型与方法**：
- `StageRuntimeContext`：单个 Stage 的完整上下文
  - `stage_goal` / `stage_domain`：Stage 元数据
  - `resolved_inputs`：已解析的 inputs（Literal 原样 + FromStage 注入实际值）
  - `previous_stage_outputs`：前序 Stage 的 outputs（stage_id → outputs map）
  - `available_tools`：完整工具列表
- `StageRuntimeContext::resolve_inputs(stage, previous_outputs)` → 解析 `TaskStage.inputs`：
  - `Literal { value }` → `value` 原样返回
  - `FromStage { stage_id, output_name }` → 查 `previous_outputs[stage_id][output_name]`
- `StageRuntimeContext::to_step_context(step, previous_step_outputs)` → 转成 `StepContext` 给 ReactAgent

**`InputSource` 解析规则**（来自 `task_planner_agent::types`）：

```rust
// 字面量
{ "kind": "literal", "value": "https://baidu.com" }
  → resolved_inputs["url"] = "https://baidu.com"

// 引用前置 stage 输出
{ "kind": "from_stage", "stage_id": "stage-1", "output_name": "topic" }
  → resolved_inputs["topic"] = previous_outputs["stage-1"]["topic"]
```

**错误处理**：
- `FromStage` 引用不存在的 stage_id → `PipelineError::InputResolution`
- `FromStage` 引用不存在的 output_name → `PipelineError::InputResolution`

### 3.5 `scheduler.rs` — DAG 调度器

**职责**：根据 TaskPlan / ExecutionPlan 的 `depends_on` 字段做拓扑调度。

**关键类型与方法**：
- `StageScheduler`：维护 stage DAG 的状态
  - `stages: HashMap<stage_id, TaskStage>`：所有 stage
  - `completed: HashSet<String>`：已成功的 stage
  - `failed: HashSet<String>`：已失败的 stage（含被级联阻塞的）
- `StageScheduler::new(task_plan)` → 构造时校验 DAG（依赖不存在的 stage_id 报 `InvalidDag`）
- `ready_stages()` → 返回依赖已满足且未处理完的 stage
- `mark_completed(stage_id)` → 标记成功
- `mark_failed(stage_id)` → 标记失败，**级联标记所有被阻塞的下游 stage**
- `is_all_done()` → 所有 stage 都已处理
- `StageScheduler::topological_steps(plan)` → 对 ExecutionPlan.steps 做 Kahn 拓扑排序

**级联失败规则**：
```text
a → b → c
a → d

如果 a 失败：
- mark_failed("a") 返回 ["b", "d"]
- b、d 自动标记为 failed（被阻塞）
- c 也被级联标记为 failed（依赖 b）
```

### 3.6 `replan.rs` — 重规划策略

**职责**：决定"什么时候该放弃 / 该重试"。

**关键类型**：
- `ReplanPolicy`：`max_stage_replan`（Layer 2）+ `max_task_replan`（Layer 3）
- `ReplanDecision`：`ReplanStage` / `ReplanTask` / `GiveUp`
- `decide_stage_replan(policy, attempt, failed_step_order)` → Layer 2 决策
- `decide_task_replan(policy, task_attempt, from_stage_id)` → Layer 3 决策

**决策规则**（与 ARCHITECTURE.md §11 一致）：

| 层 | 触发条件 | 决策函数 | 决策结果 |
|---|---|---|---|
| Layer 1 | Tool Call 失败 | ReactAgent 内部 | React Agent 自处理（Executor 不感知） |
| Layer 2 | Execution Step 失败 | `decide_stage_replan` | `attempt <= max_stage_replan` → 重试，否则 `GiveUp` |
| Layer 3 | Task Stage 失败 | `decide_task_replan` | `task_attempt <= max_task_replan` → `ReplanTask`，否则 `GiveUp` |

### 3.7 `executor.rs` — 主体

**职责**：把上述所有模块粘合起来，对外暴露 `TaskPipelineExecutor`。

**关键结构**：
```rust
pub struct TaskPipelineExecutor {
    provider: Arc<dyn LlmProvider>,                  // 注入的 LLM 后端
    react_agent: Arc<dyn ReactAgent<Output = Value>>, // 注入的执行 Agent
    event_tx: broadcast::Sender<TaskExecutionEvent>,  // 内部 broadcast 通道
}
```

**关键方法**：

| 方法 | 用途 |
|---|---|
| `new(provider)` | 构造（使用 DefaultReactAgent 桩） |
| `with_react_agent(agent)` | 注入自定义 React Agent |
| `subscribe()` | 拿 `broadcast::Receiver`（**必须在 run() 之前调用**） |
| `run(request) -> JoinHandle` | 启动执行，异步 await 拿最终结果 |
| `execute(request)` | 内部主循环（私有） |
| `execute_stage(stage, ...)` | 执行单个 Stage（含 Layer 2 replan 循环，私有） |
| `replan_task(request, from_stage_id, completed)` | Layer 3 task 重规划（私有） |

**主循环结构**（伪代码）：
```rust
loop {  // 'task_loop: Layer 3 replan
    scheduler = StageScheduler::new(current_plan)
    while !scheduler.is_all_done() {
        for stage in ready_stages {
            record = execute_stage(stage)  // 内部 Layer 2 replan 循环
            match record.status {
                Succeeded => scheduler.mark_completed(stage.id)
                Failed => {
                    blocked = scheduler.mark_failed(stage.id)  // 级联
                    decision = decide_task_replan(policy, task_attempt, stage.id)
                    if ReplanTask { current_plan = replan_task(...); continue 'task_loop }
                }
            }
        }
    }
    break
}
```

---

## 四、关键类型协作图

```text
                    TaskExecutionRequest
                            │
                            ▼
   ┌────────────────────────────────────────────────┐
   │           TaskPipelineExecutor                  │
   │  ┌──────────────┐  ┌──────────────┐            │
   │  │   provider   │  │ react_agent  │            │
   │  │  (LLM 后端)  │  │  (执行 Agent)│            │
   │  └──────┬───────┘  └──────┬───────┘            │
   │         │                 │                    │
   │         ▼                 ▼                    │
   │   TaskPlannerAgent   ReactAgent.run_step()    │
   │   ExecutionPlannerAgent                       │
   │         │                 │                    │
   │         ▼                 ▼                    │
   │    TaskPlan         StepExecutionResult       │
   │    ExecutionPlan         │                    │
   │         │                │                    │
   │         ▼                ▼                    │
   │   StageScheduler   StepExecutionRecord        │
   │         │                │                    │
   │         └────────┬───────┘                    │
   │                  ▼                            │
   │         StageExecutionRecord                  │
   │                  │                            │
   │                  ▼                            │
   │         TaskExecutionResult                   │
   └────────────────────────────────────────────────┘
                          │
                          ▼
                  broadcast::Sender ──────► 多个 Receiver
                                                │
                                                ▼
                                       service 层 emit 到 Tauri
```

---

## 五、事件流时序

完整的事件发送顺序（一次典型成功执行）：

```text
1. TaskStarted { request_id, user_request, timestamp_ms }
2. TaskPlanned { request_id, task_plan }

3. (per ready stage) StageStarted { stage_id, goal, domain, attempt: 1 }
4. (per ready stage) ExecutionPlanned { stage_id, execution_plan }
5. (per step) StepStarted { stage_id, step_order, goal, expected_tool }
6. (per step) StepFinished { step_order, success: true, output_preview, error: null }

7. StageFinished { stage_id, outputs, duration_ms }

... (重复 3-7 for each stage)

8. TaskFinished { result }
```

失败路径：

```text
1-4. 同上

5'. StepStarted { step_order, goal, ... }
6'. StepFinished { success: false, error: "..." }

7'. StageReplanning { attempt: 2, failed_step_order: N }   ← 第一次重试
5''. StepStarted { ... }
6''. StepFinished { success: true, ... }
8'. StageFinished { ... }

或耗尽重试：

7''. StageFailed { error, replanned_times: N }
9'. TaskFailed { error, partial_result }
```

---

## 六、错误处理分层

参考 ARCHITECTURE.md §11。Executor 只感知 Layer 2 / Layer 3：

| 层 | 范围 | 决策点 | 实现位置 |
|---|---|---|---|
| Layer 1 | 单个 Tool Call 失败 | React Agent 内部 | ReactAgent 实现（**不在 Executor**） |
| Layer 2 | Execution Step 失败 | `decide_stage_replan` | executor.rs::execute_stage |
| Layer 3 | Task Stage 失败 | `decide_task_replan` | executor.rs::execute |

**上报原则**（与 ARCHITECTURE 一致）：
```text
React Agent  → Execution Planner:  "Step X 失败，原因：搜索框无法定位"
Execution Planner → Task Planner:  "Stage Y 失败，无法完成搜索操作"
Task Planner → User:               "任务部分失败：搜索结果提取未完成"
```

Executor 在事件中**只传递信号，不传递细节**（避免上层被实现细节淹没）。

---

## 七、使用示例

### 7.1 最小调用（service 层）

```rust
use std::sync::Arc;
use tokio::sync::broadcast;
use crate::provider::llm::planner::pipeline_executor::{
    TaskPipelineExecutor, TaskExecutionRequest, TaskExecutionResult,
};

pub async fn run_task(
    provider: Arc<dyn LlmProvider>,
    request: TaskExecutionRequest,
) -> Result<TaskExecutionResult, String> {
    let executor = Arc::new(TaskPipelineExecutor::new(provider));

    // 1. 必须在 run() 之前订阅，否则错过 TaskStarted / TaskPlanned
    let mut rx = executor.subscribe();

    // 2. 后台消费事件
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // 转发到 Tauri / 写日志 / 持久化等
            tracing::info!(?event, "task progress");
        }
    });

    // 3. 启动执行，await 拿最终结果
    let handle = executor.run(request);
    handle.await.map_err(|e| e.to_string())
}
```

### 7.2 注入自定义 React Agent

```rust
use std::sync::Arc;
use async_trait::async_trait;
use crate::provider::llm::planner::react_agent::{ReactAgent, types::{StepContext, StepExecutionResult}};
use crate::provider::llm::planner::execution_planner_agent::types::ExecutionStep;
use crate::provider::llm::error::LlmError;

struct MyReactAgent {
    provider: Arc<dyn LlmProvider>,
}

#[async_trait]
impl ReactAgent for MyReactAgent {
    type Output = serde_json::Value;

    async fn run_step(
        &self,
        step: &ExecutionStep,
        context: &StepContext,
    ) -> Result<StepExecutionResult<serde_json::Value>, LlmError> {
        // 真实 Thought→Action→Observe 循环
        todo!()
    }
}

let executor = TaskPipelineExecutor::new(provider)
    .with_react_agent(Arc::new(MyReactAgent { provider: provider.clone() }));
```

### 7.3 Service 层订阅（Tauri 集成）

```rust
// services/llm/task_pipeline_service.rs
use tauri::{AppHandle, Emitter};

pub struct TaskPipelineService {
    executor: Arc<TaskPipelineExecutor>,
    app_handle: AppHandle,
}

impl TaskPipelineService {
    pub async fn execute(&self, request: TaskExecutionRequest) -> Result<TaskExecutionResult, String> {
        let mut rx = self.executor.subscribe();
        let app = self.app_handle.clone();
        let request_id = request.request_id.clone();

        // 后台消费 → emit 到 Tauri
        tokio::spawn(async move {
            const TAURI_CHANNEL: &str = "planner://task-progress";
            while let Ok(event) = rx.recv().await {
                let _ = app.emit(TAURI_CHANNEL, &event);
            }
        });

        self.executor.clone().run(request).await
            .map_err(|e| e.to_string())
    }
}
```

**注意**：`pipeline_executor` 内部**不包含** `tauri::*` 任何引用，service 层才是 Tauri 集成的边界。

---

## 八、扩展点

### 8.1 新增领域

无需修改 `pipeline_executor`。在 [planning_rules/](../planner/planning_rules/) 下添加新规则文件 + 在 `execution_planner_agent/agent.rs::load_planning_rules` 注册即可。

### 8.2 自定义失败处理

替换 `ReplanPolicy` 默认值或注入自定义 `decide_*` 决策函数。当前 `ReplanPolicy` 是简单 Copy struct，可整体替换为更复杂的状态机。

### 8.3 自定义进度事件

- 增加事件变体：编辑 `event.rs` 的 `TaskExecutionEvent` 枚举，**注意**：
  - 所有变体必须可 `Clone + Serialize`
  - 保持 `#[serde(tag = "kind", rename_all = "snake_case")]` 不变（前端依赖此格式）
- 调整事件粒度：在 `executor.rs::execute_stage` 中按需增删 `event_tx.send(...)` 调用

### 8.4 自定义 React Agent

实现 `ReactAgent` trait 后通过 `TaskPipelineExecutor::with_react_agent()` 注入。Executor 不感知 React Agent 的具体实现。

---

## 九、相关文档

- [planner/docs/ARCHITECTURE.md](../planner/docs/ARCHITECTURE.md) — 整体三层规划架构（含失败处理分层）
- [planner/docs/01-task-planner-prompt.md](../planner/docs/01-task-planner-prompt.md) — Task Planner 提示词
- [planner/docs/02-execution-planner-prompt.md](../planner/docs/02-execution-planner-prompt.md) — Execution Planner 提示词
- `task_planner_agent/agent.rs` — Task Planner Agent 实现
- `execution_planner_agent/agent.rs` — Execution Planner Agent 实现
- `react_agent/mod.rs` — React Agent Trait 契约
