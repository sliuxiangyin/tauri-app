# Universal Agent 三层规划架构设计（V3）

# 一、设计目标

构建一个可扩展、多领域支持的 Agent Planning Framework。

核心原则：

> **Planning Rules 属于领域知识（Domain Knowledge），不是任务知识（Task Knowledge）。**

Task Planner 知道"在哪个领域做事"（domain），但不知道这个领域的执行细节（Planning Rules）。领域执行细节由 Execution Planner 加载 Domain Strategy 获得。

整个系统采用三层规划：

```
User Request
      │
      ▼
 Task Planner
      │
 Task Stage (含 domain)
      │
      ▼
 Domain Router → 加载 Planning Rules
      │
      ▼
Execution Planner (+ Planning Rules)
      │
Execution Step
      │
      ▼
 React Agent
      │
 Tool Call
```

遵循：

* Task Planner：规划任务阶段 + 判定领域
* Domain Router：加载对应领域的 Planning Rules
* Execution Planner：在领域规则约束下规划执行步骤
* React Agent：执行工具

每层职责单一，互不重叠。

---

# 二、整体架构

```
User
   │
   ▼
Task Planner（LLM）
   │
   │ 输出：Task Plan
   ▼
Stage
{
    goal              // 阶段目标
    expected_output   // 期望产出
    domain            // 所属领域（由 Task Planner 判定）
    depends_on        // 依赖的前置 Stage（DAG）
}
   │
   ▼
Domain Router（程序）
   │ 根据 Stage.domain 加载对应 Planning Rules
   │
   ├── browser → Browser Planning Rules
   ├── file    → File Planning Rules
   ├── adb     → ADB Planning Rules
   ├── office  → Office Planning Rules
   └── ...
   │
   ▼
Execution Planner（LLM + Planning Rules）
   │
   ▼
Execution Steps
   │
   ▼
React Agent（LLM）
   │
   ▼
Tool Call
```

自顶向下逐层细化（Progressive Refinement）。

---

# 三、Task Planner

## 职责

Task Planner 负责回答两个问题：

> 1. 为完成用户目标，需要经历哪些任务阶段（Task Stage）？
> 2. 这些阶段属于哪个领域（Domain）？

## 为什么 Task Planner 输出 domain？

一个高层任务天然属于同一个领域。例如"在百度搜索 AI 新闻并提取摘要"全程是 Browser 操作，"读取配置文件并写入数据库"全程是 File + Database 操作。不会出现一个 Task 中 Stage1 是 Browser、Stage2 是 ADB 的情况——那本质上是两个独立任务。

因此，domain 是任务级别的属性，Task Planner 在分解 Stage 时自然知道领域。

Task Planner 不关心：

* Tool
* Selector
* XPath
* API
* 文件路径
* DOM
* 点击
* 输入
* Planning Rules

这些全部属于执行层。

---

## 输入

```
User Request

Conversation Context

History（可选）

可用领域列表（available domains）
```

---

## 输出

Task Plan，包含一组 Stage，Stage 之间通过 `depends_on` 形成 DAG：

```
Task Plan
│
├── Stage1: { goal: "打开浏览器并导航到搜索页", domain: "browser", depends_on: [] }
│
├── Stage2: { goal: "输入关键词执行搜索", domain: "browser", depends_on: [Stage1] }
│
├── Stage3: { goal: "提取前三条搜索结果", domain: "browser", depends_on: [Stage2] }
│
└── Stage4: { goal: "整理结果生成摘要", domain: "browser", depends_on: [Stage3] }
```

依赖支持多依赖（DAG）：

```
Stage1: 读取配置文件      ───── depends_on: []
Stage2: 解析配置          ───── depends_on: [Stage1]
Stage3: 验证数据库连接     ───── depends_on: [Stage1]
Stage4: 生成报告          ───── depends_on: [Stage2, Stage3]
```

---

## Stage 定义

Stage 表示：

> 一个独立业务目标。

每个 Stage 有明确的目标（goal）和期望产出（expected_output），属于同一个领域。

正确：

```
读取配置文件

执行搜索

验证连接

保存报告
```

错误：

```
点击按钮

输入文本

读取DOM

获取Selector
```

这些属于执行细节，应由 Execution Planner 产出。

---

## 系统提示词

详见 [01-task-planner-prompt.md](./01-task-planner-prompt.md)

---

# 四、Domain Router

## 职责

Domain Router 是纯程序组件，不调用 LLM。

Task Planner 已经给出了每个 Stage 的 `domain`，Router 只需：

> 根据 domain 加载对应的 Planning Rules，注入到 Execution Planner。

```
Stage { domain: "browser" }
        │
        ▼
  Domain Router
        │
        ▼
  Browser Planning Rules
        │
        ▼
  Execution Planner (+ Rules)
```

Router 不识别领域、不生成 Plan、不修改 Stage。只做规则加载。

---

# 五、Execution Planner

## 职责

Execution Planner 回答：

> 在给定领域的约束下，如何完成一个 Task Stage？

输入：

```
Task Stage（goal + expected_output + domain）

Domain Planning Rules（由 Domain Router 注入）

Tool List（该领域可用工具）

运行环境

前序 Stage 输出
```

输出：

细粒度的 Execution Step 序列。

例如，对于 Stage "在搜索框中输入关键词并执行搜索"：

```
Step1: 定位搜索输入框
Step2: 输入关键词
Step3: 定位搜索按钮
Step4: 点击搜索按钮
Step5: 等待搜索结果页面加载
```

Execution Planner 已经知道可用 Tool、Planning Rules，但仍然不直接调用 Tool——它只规划到步骤级别。

注意：如果没有 Planning Rules（简单场景），Execution Planner 仍然可以工作，只是缺少领域最佳实践的指引。

---

## Tool List 信息粒度

Execution Planner 接收的 Tool List（对应提示词中的 `{{AVAILABLE_TOOLS}}`）只提供**工具名称 + 工具类别 + 一句话能力描述**，不包含完整参数 schema。

原因：Execution Planner 输出的是 `expected_tool_category`（工具类别），具体工具选择和参数构造是 React Agent 的职责。过多的参数细节会模糊规划层的职责边界。

格式示例：

```
- browser.click (browser_interaction): 点击页面上的指定元素
- browser.type (browser_input): 在输入框中输入文本
- browser.snapshot (browser_extraction): 获取当前页面快照/DOM
- fs.read (fs_read): 读取指定文件的内容
- fs.write (fs_write): 向指定文件写入内容
- adb.tap (adb_interaction): 点击设备屏幕上的指定坐标
```

| 信息 | 是否包含 | 说明 |
|---|---|---|
| 工具名 | 是 | 便于引用和识别 |
| 工具类别 | 是 | 与 ExecutionStep 的 `expected_tool_category` 对齐 |
| 能力描述 | 是 | 理解该工具"能做什么"，判断步骤可行性 |
| 参数 schema | 否 | 属于 React Agent 层，Execution Planner 不关心 |
| 返回值格式 | 否 | 属于 React Agent 层 |

---

## 系统提示词

详见 [02-execution-planner-prompt.md](./02-execution-planner-prompt.md)

---

# 六、React Agent

## 职责

React Agent 接收 Execution Step，负责**真正执行工具**。

React Agent 是执行层级的最底层，负责：

- **工具选择**：从 Tool List 中选择最适合完成当前 Step 的工具
- **参数构造**：根据 Step 描述 + 上下文构造工具参数
- **工具调用**：执行 Tool Call
- **结果验证**：观察结果，判断 Step 是否完成
- **内部重试**：Step 内失败时换工具/换参数重试

流程：

```
接收 Execution Step（如"定位搜索输入框"）
        │
        ▼
      Thought（分析当前页面状态）
        │
        ▼
    选择 Tool（browser.snapshot）
        │
        ▼
    调用 Tool
        │
        ▼
   观察结果（分析 DOM，找到输入框）
        │
        ▼
    继续执行（browser.click 聚焦输入框）
        │
        ▼
    Step 完成
```

React Agent 的一个 Execution Step 可能对应多次 Tool Call，这是正常的——它自己决定需要多少次工具调用才能完成当前 Step。

---

# 七、Domain Strategy（领域策略）

Execution Planner 本身只有一套框架。

真正不同的是 Domain Strategy——每个领域的 Planning Rules。

例如：

```
Browser      → Browser Planning Rules
File         → File Planning Rules
ADB          → ADB Planning Rules
Office       → Office Planning Rules
Database     → Database Planning Rules
HTTP         → HTTP Planning Rules
Terminal     → Terminal Planning Rules
```

每个 Domain 的 Planning Rules 描述的是该领域的执行最佳实践（How），由系统维护，Evolution Planner 在执行时加载。

---

# 八、Planning Rules 属于哪里？

Planning Rules：

属于：**Domain Strategy**（注入给 Execution Planner）。

不属于：**Task Planner**。

原因：

* Task Planner 负责 **What**（做什么阶段、什么领域）
* Execution Planner 负责 **How**（在领域约束下怎么做）
* Planning Rules 描述的是 **How**

所以 Planning Rules 必须属于 Execution Planner 侧。

---

# 九、Domain Planning Rules

## Browser Planning Rules

```
页面交互前应确认页面已加载。
输入文本前应定位输入框。
点击按钮前应定位按钮。
页面跳转后应等待页面稳定。
元素未知时生成 Exploratory Step。
可优先复用已有页面信息，避免重复截图。
```

---

## File Planning Rules

```
读取前确认文件存在。
写入前确认目录存在。
JSON 文件需要解析。
写入完成后确认成功。
```

---

## ADB Planning Rules

```
确认设备在线。
确认当前页面。
页面未知时先截图。
点击前定位控件。
页面跳转后等待稳定。
```

---

# 十、上下文流转

```
User
    │
    ▼
Task Planner
    │
Task Plan（Stages + domain + DAG）
    │
    ▼
Domain Router（加载 Planning Rules）
    │
    ▼
Execution Planner（+ Planning Rules）
    │
Execution Steps
    │
    ▼
React Agent
    │
Observation
    │
Tool Result
```

上下文原则：

* **正常流转**：向下传递（Task Plan → Execution Steps → Tool Calls）
* **失败上报**：向上传递失败信号，但不传递执行细节（见下章）
* **领域知识**：Planning Rules 从 Domain Router 侧向注入，不经过 Task Planner

---

# 十一、失败处理分层策略

每一层有自己的重试边界，本级耗尽后才向上级报告：

```
┌─────────────────────────────────────────────────────┐
│ Tool Call 失败                                      │
│   → React Agent 内部重试（换参数 / 换工具 / 重试）      │
│   → 仍失败：该 Execution Step 标记为失败              │
│                                                     │
│ Execution Step 失败                                 │
│   → Execution Planner replan 当前 Stage              │
│     （重新规划该 Stage 的剩余步骤）                     │
│   → 仍失败：该 Stage 标记为失败                       │
│                                                     │
│ Task Stage 失败                                     │
│   → 若不影响后续 Stage（非依赖）：跳过，继续执行          │
│   → 若被后续 Stage 依赖：Task Planner 重规划           │
│     （重新分解剩余任务）                                │
│   → 仍失败：整个 Task 失败，向用户报告                  │
└─────────────────────────────────────────────────────┘
```

### 层级 1：Tool Call 失败 → React Agent 重试

React Agent 在 Thought→Action→Observe 循环中，如果单个 Tool Call 失败：
1. 分析失败原因
2. 尝试换参数重试（如超时 → 加大超时时间）
3. 尝试换工具重试（如 `browser.click` 失败 → 尝试 `browser.click_js`）
4. 重试次数耗尽 → 该 Execution Step 标记为失败，上报 Execution Planner

### 层级 2：Execution Step 失败 → Execution Planner replan

Execution Planner 收到 Step 失败信号后：
1. 重新规划当前 Stage 的剩余步骤
2. 可能生成替代步骤（如原 Step 是 "点击登录按钮"，替换为 "通过 API 登录"）
3. Replan 次数耗尽 → 该 Stage 标记为失败，上报 Stage Scheduler

### 层级 3：Task Stage 失败 → 上层调度

Stage Scheduler（程序组件）根据 DAG 判断：
- 该 Stage 不被任何后续 Stage 依赖 → 跳过，继续执行
- 该 Stage 被后续 Stage 依赖 → 通知 Task Planner 重规划剩余 Task
- 最后一个 Stage 失败且无法替换 → 整个 Task 失败，返回给用户

### 上报原则

失败上报只传递**信号**，不传递执行细节：

```
React Agent → Execution Planner：  "Step X 失败，原因：搜索框无法定位"
Execution Planner → Task Planner："Stage Y 失败，无法完成搜索操作"
Task Planner → User：             "任务部分失败：搜索结果提取未完成"
```

上层不需要知道下层具体试了哪些工具、什么参数，只需要知道"什么失败了、为什么"以便做出决策。

---

# 十二、类型定义

## Task Stage

表示完成任务必须经历的业务阶段。

```typescript
interface TaskStage {
  id: string                    // 阶段唯一标识
  goal: string                  // 阶段目标（业务语义）
  expected_output: string       // 期望产出描述
  domain: string                // 所属领域（browser | file | adb | office | database | http | terminal）
  depends_on: string[]          // 依赖的前置 Stage ID 列表（DAG）
}
```

特点：

* 面向业务目标（不关心实现）
* 属于同一个领域
* 支持 DAG 依赖编排
* 生命周期覆盖整个任务

---

## Execution Step

表示完成一个 Stage 所需的执行步骤。

```typescript
interface ExecutionStep {
  order: number                 // 步骤序号
  goal: string                  // 步骤目标（执行语义，如"定位搜索输入框"）
  depends_on: number[]          // 依赖的前置 Step 序号
  expected_tool_category: string // 预期使用的工具类别（如 browser_input、fs_read），非具体工具名
}
```

特点：

* 面向执行（细粒度）
* 包含依赖关系
* 可映射到工具类别，非具体工具名
* 具体工具选择留给 React Agent

---

## Tool Call

一次具体工具调用，由 React Agent 决定。

```
browser.navigate    → { url: "https://example.com" }
browser.click       → { selector: "#search-btn" }
browser.input       → { selector: "#kw", text: "AI新闻" }
fs.read             → { path: "/data/config.json" }
adb.tap             → { x: 100, y: 200 }
```

---

# 十三、最终职责划分

| 模块                | 输入                              | 输出                    | 关心 Tool | 关心 Domain | 关心 Planning Rules |
| ------------------- | ------------------------------- | ----------------------- | -------- | ---------- | ----------------- |
| Task Planner        | User Request + domains          | TaskStage[] (含 domain) | 否        | 是（判定）    | 否                 |
| Domain Router       | Stage.domain                    | Planning Rules          | 否        | 是（匹配）    | 否                 |
| Execution Planner   | Stage + Rules + Tool List       | ExecutionStep[]         | 是        | 是          | 是                 |
| React Agent         | ExecutionStep                   | ToolCall[]              | 是        | 是          | 否                 |

整个架构遵循：

**Task Planner 负责 What + Which Domain。**

**Domain Router 负责加载 Rules。**

**Execution Planner 负责 How（细粒度）。**

**React Agent 负责 Do（工具选择 + 执行）。**

---

# 十四、扩展性

新增领域（如 Bluetooth、Camera）时：

1. 新增一个 Domain Planning Rules 文件（如 `bluetooth_rules.md`）
2. 在 Domain Router 注册表中添加映射：`"bluetooth" → BluetoothRules`
3. 将 bluetooth 加入 Task Planner 的 available domains 列表

无需修改 Task Planner、Execution Planner、React Agent 的核心逻辑。架构天然支持水平扩展。

---

# 十五、相关文档

- [01-task-planner-prompt.md](./01-task-planner-prompt.md) - Task Planner Agent 系统提示词
- [02-execution-planner-prompt.md](./02-execution-planner-prompt.md) - Execution Planner Agent 系统提示词
