//! 计划生成提示词模块
//!
//! 职责：
//! - 接收意图分析（intent_prompt）输出的 `reasoning`，将其展开为详细的执行计划（PlanStep 列表）。
//! - 输出 JSON 格式的步骤列表，每个步骤描述**宏观意图**（如"完成搜索"），具体动作由 ReAct 循环在执行时动态生成。
//!
//! ## 在整体流程中的位置
//! ```text
//! 用户请求
//!   ↓
//! 1. intent_prompt → { need_agent, reasoning }
//!   ↓ (need_agent = true)
//! 2. plans_prompt  → { steps: [PlanStep, ...] }   ← 本模块
//!   ↓
//! 3. PlanExecutor  依次执行每个 PlanStep（ReAct 循环）
//! ```
//!
//! ## 字段语义
//! - `PlanStep`：一个可执行的宏观意图单元；具体子动作（SubAction）由 ReAct 在执行时填充。
//! - `SubAction`：ReAct 模式下的具体操作单元（tool_name + parameters + output）。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::provider::llm::agent::types::StepType;
use crate::provider::llm::error::LlmError;

// ──────────────────────────────────────────────────────────────
// 数据结构
// ──────────────────────────────────────────────────────────────

/// 计划步骤 - 代表一个可执行的宏观意图单元
///
/// 描述高层目标（如"完成搜索"、"提取结果"），具体操作由 ReAct 执行时动态生成 SubAction
///
/// ## 字段语义
/// - `expected_output`：本步骤**成功后的产物**（如 JSON、文件路径、报告）
/// - `success_criteria`：**任务完成判定标准**（多条件数组，最后一步必须非空）
/// - `depends_on`：依赖的前置步骤序号列表（**支持多依赖**）
/// - `input`：本步骤输入字段的引用（如 `"{{step_1.user_id}}"`），用于 DAG 编排
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// 步骤序号（从 1 开始，连续递增）
    pub order: u8,
    /// 步骤类型
    ///
    /// - `deterministic`：工具和参数在计划阶段已知，可直接执行
    /// - `exploratory`：需要在执行时根据上下文决定工具和参数（ReAct）
    #[serde(default)]
    pub step_type: StepType,
    /// 本步骤目标描述（高层意图，如"完成搜索"）
    pub step_goal: String,
    /// 步骤成功后的**产物**（如 JSON、文件路径、报告）
    #[serde(default)]
    pub expected_output: Option<String>,
    /// 依赖的前置步骤序号列表（支持多依赖）
    ///
    /// 序列化时输出数组；反序列化时**兼容**以下三种 LLM 格式：
    /// - `5`（单个数字） → `[5]`
    /// - `[1, 2]`（数组） → `[1, 2]`
    /// - `null` / 缺失 → `[]`
    #[serde(
        default,
        serialize_with = "serialize_depends_on",
        deserialize_with = "deserialize_depends_on"
    )]
    pub depends_on: Vec<u8>,
    /// 当前步骤所需的输入字段引用列表
    ///
    /// 元素格式示例：`"{{step_1.user_id}}"`、`"{{step_2.result.url}}"`、`"{{step_1.output}}"`
    #[serde(default)]
    pub input: Vec<String>,
    /// 步骤成功判定标准（多条件数组）
    ///
    /// 最后一个步骤**必须**包含明确的成功判定标准。
    #[serde(default)]
    pub success_criteria: Vec<String>,
    /// 子动作列表
    ///
    /// - deterministic 步骤：≥1 条 SubAction
    /// - exploratory 步骤：必须为空（由 ReAct 在执行时动态填充）
    #[serde(default)]
    pub actions: Vec<SubAction>,
}

/// `depends_on` 字段的序列化函数
///
/// 始终序列化为 JSON 数组（即使是空也输出 `[]`）
fn serialize_depends_on<S>(deps: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    deps.serialize(serializer)
}

/// `depends_on` 字段的兼容反序列化函数
///
/// 接受以下格式：
/// - `null` → `Vec::new()`
/// - 字段缺失 → `Vec::new()`
/// - 单个数字 `5` → `vec![5]`
/// - 数组 `[1, 2]` → 原样解析
fn deserialize_depends_on<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // 只接受数组格式
    let arr: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;

    let mut deps = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        match item {
            serde_json::Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    if u > u8::MAX as u64 {
                        return Err(Error::custom(format!(
                            "depends_on[{}] = {} exceeds u8::MAX",
                            idx, u
                        )));
                    }
                    deps.push(u as u8);
                } else {
                    return Err(Error::custom(format!(
                        "depends_on[{}] must be a non-negative integer, got: {}",
                        idx, n
                    )));
                }
            }
            _ => {
                return Err(Error::custom(format!(
                    "depends_on[{}] must be an integer, got: {}",
                    idx, item
                )));
            }
        }
    }
    Ok(deps)
}

/// 子动作 - ReAct 模式下的具体操作单元
///
/// 由 LLM 在执行时动态决定并执行，记录到 actions 列表中供后续步骤参考
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAction {
    /// 动作序号（从 1 开始）
    pub order: u8,
    /// 工具名称（完整格式，如 "mcp__browser__navigate"）
    pub tool_name: String,
    /// 工具调用参数（JSON 对象）
    pub parameters: serde_json::Value,
    /// 动作输出结果（执行后填充）
    pub output: Option<String>,
}

/// 计划生成响应 - LLM 输出的整体计划
///
/// 对应 prompt 输出格式（见 [`plans_system_prompt`]）：
/// ```json
/// {
///   "steps": [
///     {
///       "order": 1,
///       "step_type": "exploratory",
///       "step_goal": "...",
///       "expected_output": "...",
///       "depends_on": []
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlansResponse {
    /// 计划步骤列表
    pub steps: Vec<PlanStep>,
}

// ──────────────────────────────────────────────────────────────
// PlanStep 构造器
// ──────────────────────────────────────────────────────────────

#[allow(dead_code)]
impl PlanStep {
    /// 创建新的计划步骤（默认确定性类型）
    pub fn new(order: u8, tool_name: impl Into<String>, step_goal: impl Into<String>) -> Self {
        // Deterministic 步骤：将初始工具转换为第一个 SubAction
        let action = SubAction {
            order: 1,
            tool_name: tool_name.into(),
            parameters: serde_json::json!({}),
            output: None,
        };
        Self {
            order,
            step_type: StepType::Deterministic,
            step_goal: step_goal.into(),
            expected_output: None,
            depends_on: Vec::new(),
            input: Vec::new(),
            success_criteria: Vec::new(),
            actions: vec![action],
        }
    }

    /// 创建探索性步骤
    pub fn exploratory(order: u8, step_goal: impl Into<String>) -> Self {
        Self {
            order,
            step_type: StepType::Exploratory,
            step_goal: step_goal.into(),
            expected_output: None,
            depends_on: Vec::new(),
            input: Vec::new(),
            success_criteria: Vec::new(),
            actions: Vec::new(), // ReAct 执行时动态填充
        }
    }

    /// 设置期望输出（步骤成功后的产物）
    pub fn with_expected_output(mut self, output: impl Into<String>) -> Self {
        self.expected_output = Some(output.into());
        self
    }

    /// 添加一个依赖的前置步骤（链式调用可多次添加，构建多依赖）
    pub fn with_dependency(mut self, depends_on: u8) -> Self {
        self.depends_on.push(depends_on);
        self
    }

    /// 一次性设置多个依赖的前置步骤
    pub fn with_dependencies<I>(mut self, deps: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<u8>,
    {
        self.depends_on.extend(deps.into_iter().map(Into::into));
        self
    }

    /// 设置步骤的输入字段引用列表
    ///
    /// 元素格式示例：`"{{step_1.user_id}}"`、`"{{step_2.result.url}}"`、`"{{step_1.output}}"`
    ///
    /// 同时接受 `&str` 与 `String` 两种类型的输入。
    pub fn with_input<I>(mut self, input: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.input = input.into_iter().map(Into::into).collect();
        self
    }

    /// 设置步骤成功判定标准（多个条件）
    ///
    /// 同时接受 `&str` 与 `String` 两种类型的输入。
    pub fn with_success_criteria<I>(mut self, criteria: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.success_criteria = criteria.into_iter().map(Into::into).collect();
        self
    }
}

// ──────────────────────────────────────────────────────────────
// 提示词与消息构建
// ──────────────────────────────────────────────────────────────

/// 计划生成系统提示词
///
/// 接收意图分析阶段输出的 `reasoning`，将其分解为**宏观意图步骤**列表。
///
/// ## 设计原则（场景无关）
/// - **不绑定特定领域**：本提示词同时覆盖浏览器自动化、HTTP/文件/数据库操作、
///   代码生成、数据分析、纯 LLM 推理等场景，**不会**让 LLM 倾向把所有任务拆成浏览器操作。
/// - **每个步骤是完整的宏观意图**：例如"获取用户列表"、"读取 config.json"，
///   而不是"调用 http__get 发起 GET 请求"（应作为 actions 元素）。
/// - **根据路径明确程度区分 step_type**：
///   - `deterministic`（路径明确）：必须给出**完整的 `actions` 列表**，每条 action 包含
///     `tool_name`（遵循 `{server}__{tool}` 命名）+ `parameters`（JSON 对象），执行时按序调用即可。
///   - `exploratory`（路径不明确）：`actions` 留空 `[]`，由 ReAct 循环在执行时根据
///     运行时上下文（DOM/数据/外部状态）动态决策。
/// - **关注步骤间依赖与终止条件**：通过 `depends_on` 和 `expected_output` 显式表达。
/// - **步骤数量适中**：3-8 个为佳，过细会增加执行开销，过粗会丢失关键决策点。
pub fn plans_system_prompt() -> &'static str {
    r#"你是一个通用任务计划生成助手（Universal Task Planner）。
你的职责是根据输入内容分析任务目标、依赖关系和不确定因素，并生成可执行的高层任务计划（Execution Plan）。
你只负责规划，不负责执行。
---
# 输入

输入固定为：

```json
{
  "content": "..."
}
```

其中：

* `content` 是任务上下文。
* `content` 可能包含用户原始请求、意图分析结果、推理结果、约束条件、已有信息或其他上下文。
* `content` 的格式不固定。

可能是：

* 自然语言
* Markdown
* XML
* JSON
* 混合文本

你必须自行从 content 中识别：

1. 任务目标
2. 主要执行阶段
3. 步骤依赖关系
4. 不确定因素
5. 最终交付物

不要要求 content 具有固定结构。

---

# 输出

严格输出一个 JSON 对象：

```json
{
  "steps": [...]
}
```

禁止输出：

* Markdown
* 代码块
* 注释
* 解释说明
* 额外文本

输出必须是合法 JSON。

---

# PlanStep Schema

```json
{
  "order": 1,
  "step_type": "deterministic",
  "step_goal": "步骤目标",
  "expected_output": "步骤产出",
  "depends_on": [],
  "input": [],
  "success_criteria": [],
  "actions": []
}
```

---

# 字段说明

## order

类型：

```json
integer
```

要求：

* 从 1 开始
* 连续递增
* 不允许跳号

---

## step_type

类型：

```json
string
```

可选值：

```text
deterministic
reasoning
exploratory
```

默认：

```text
deterministic
```

---

## step_goal

当前步骤的高层目标。

正确示例：

```text
读取配置文件
获取用户列表
统计销售数据
生成最终报告
```

错误示例：

```text
调用 read_file
执行 SQL
点击按钮
```

工具调用属于 actions。

---

## expected_output

当前步骤完成后的预期产出。

示例：

```text
配置对象
用户列表 JSON
统计结果
Markdown 报告
文件路径
```

---

## depends_on

类型：

```json
[]
```

示例：

```json
[]
```

```json
[1]
```

```json
[1,2]
```

规则：

* 支持多个依赖
* 所有依赖序号必须小于当前步骤序号

---

## input

当前步骤所需输入。

类型：

```json
[]
```

示例：

```json
[
  "{{step_1.output}}"
]
```

```json
[
  "{{step_1.user_id}}",
  "{{step_2.token}}"
]
```

没有输入时：

```json
[]
```

---

## success_criteria

步骤成功判定标准。

类型：

```json
[]
```

示例：

```json
[
  "成功获取用户列表",
  "返回结果数量大于0"
]
```

```json
[
  "文件成功写入",
  "返回文件路径"
]
```

最后一个步骤必须包含明确的任务完成标准。

---

## actions

子动作列表。

---

# Action Schema

```json
{
  "order": 1,
  "tool_name": "server__tool",
  "parameters": {}
}
```

---

# tool_name 规范

统一格式：

```text
{server}__{tool}
```

例如：

```text
http__get
http__post

db__query
db__execute

mcp__fs__read_file
mcp__fs__write_file

mcp__browser__navigate
mcp__browser__click
mcp__browser__screenshot
```

不要编造不存在的工具。

如果无法确定具体工具，优先使用 exploratory 步骤。

---

# 参数引用规则

允许 deterministic 步骤引用前序步骤输出。

引用整个输出：

```json
{
  "content": "{{step_2.output}}"
}
```

引用字段：

```json
{
  "user_id": "{{step_1.user_id}}"
}
```

引用嵌套字段：

```json
{
  "url": "{{step_2.result.url}}"
}
```

---

# Step Type 判定规则

## deterministic

满足以下任意条件：

* 工具已知
* 调用顺序已知
* 参数结构已知
* 仅依赖结构化变量注入

即使部分参数来自前序步骤输出，只要调用路径明确，仍属于 deterministic。

示例：

```text
读取配置
获取 endpoint
调用 API
```

属于 deterministic。

---

## reasoning

用于需要模型推理、分析、归纳或内容生成的场景。

典型场景：

* 文本总结
* 信息抽取
* 内容分类
* 数据分析解释
* 报告生成
* 产品对比
* 自然语言回复生成

要求：

```json
{
  "step_type": "reasoning",
  "actions": []
}
```

reasoning 步骤必须为空 actions。

---

## exploratory

用于运行时存在不确定因素，需要动态决策。

典型场景：

* 页面结构未知
* 选择器未知
* 登录流程
* 验证码
* MFA
* 动态弹窗
* 从运行时结果中寻找目标对象
* 提取未知 ID
* 文件状态未知
* 外部环境状态未知

要求：

```json
{
  "step_type": "exploratory",
  "actions": []
}
```

exploratory 步骤必须为空 actions。

---

# Actions 规则

## deterministic

必须满足：

```text
actions.length >= 1
```

每个 action 必须包含：

```json
{
  "order": 1,
  "tool_name": "...",
  "parameters": {}
}
```

---

## reasoning

必须：

```json
[]
```

---

## exploratory

必须：

```json
[]
```

---

# 规划原则

## 原则1

步骤表达的是宏观目标。

不要把每个工具调用拆成独立步骤。

正确：

```text
读取销售数据
统计销售结果
保存统计文件
```

错误：

```text
调用 read_file
调用 group_by
调用 write_file
```

---

## 原则2

优先生成 3~8 个步骤。

任务简单时允许少于 3 个步骤。

不要过度拆分。

---

## 原则3

强依赖必须显式声明。

例如：

```json
{
  "depends_on": [1,2]
}
```

---

## 原则4

尽量将确定路径规划为 deterministic。

不要滥用 exploratory。

---

## 原则5

所有分析、归纳、总结、生成类任务优先使用 reasoning。

不要误标为 exploratory。

---

## 原则6

无法确定具体工具、具体参数或具体操作对象时，使用 exploratory。

---

# 输出校验规则

生成结果前必须满足：

1. 仅输出 JSON。
2. steps 数组不能为空。
3. order 从 1 开始连续递增。
4. depends_on 中所有序号必须存在。
5. deterministic 的 actions 不允许为空。
6. reasoning 的 actions 必须为空数组。
7. exploratory 的 actions 必须为空数组。
8. 每个 action 必须包含：

   * order
   * tool_name
   * parameters
9. parameters 必须是 JSON 对象。
10. 最后一个步骤必须包含明确的 success_criteria。
11. 输出必须是合法 JSON。
12. 不允许输出任何额外内容。

优化提示词"#
}

/// 构建计划生成的用户消息
///
/// 上游（intent_prompt 解析阶段）已将 `user_request` 与 `reasoning` 组合成完整内容，
/// 本函数只做一次纯字符串透传，避免重复解析与拼接。
///
/// ## 参数
/// - `content`：已经包含用户请求与意图分析 reasoning 的完整文本
pub fn build_plans_user_message(content: &str) -> String {
    content.to_string()
}

// ──────────────────────────────────────────────────────────────
// 响应解析
// ──────────────────────────────────────────────────────────────

/// 解析 LLM 返回的计划响应 JSON
///
/// ## 容错处理
/// - 纯 JSON 字符串
/// - Markdown 代码块包裹的 JSON
/// - JSON 中夹杂额外文本
/// - `actions` 字段缺失或非数组（按 step_type 处理）
/// - `step_type` 字段缺失（自动设为默认值 `deterministic`）
/// - `depends_on` 只接受数组格式
///
/// ## 校验规则（按 step_type 区分）
/// - **公共校验**
///   - `steps` 必须为非空数组
///   - `order` 必须从 1 开始连续递增
///   - `step_goal` 非空
///   - `depends_on` 指向的步骤序号必须小于当前 `order`（多依赖全部校验）
///   - 最后一个步骤的 `success_criteria` 必须非空（≥1 条可客观验证的判定标准）
/// - **deterministic 步骤**
///   - `actions` 必须为非空数组（至少 1 条 SubAction）
///   - 每条 SubAction 必须包含 `order` / `tool_name` / `parameters`
/// - **exploratory 步骤**
///   - `actions` 必须为空数组 `[]`（执行时由 ReAct 循环动态填充）
///   - 若 LLM 返回了非空 actions，记 `tracing::warn!` 但**不**自动清空
pub fn parse_plans_response(response: &str) -> Result<PlansResponse, LlmError> {
    // 1) 从响应中提取 JSON 子串（首尾 { } 之间），去除 Markdown 包裹或多余文本
    let json_str = match (response.find('{'), response.rfind('}')) {
        (Some(start), Some(end)) if end >= start => &response[start..=end],
        _ => response,
    };

    // 2) 解析为通用 JSON 值
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| LlmError::ParseError(format!("Failed to parse plans JSON: {}", e)))?;

    let obj = value.as_object().ok_or_else(|| {
        LlmError::ParseError("Invalid plans JSON: expected object".into())
    })?;

    // 3) 提取 steps 数组（必填且非空）
    let steps_value = obj
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| LlmError::ParseError("steps must be a non-empty array".into()))?;

    if steps_value.is_empty() {
        return Err(LlmError::ParseError("steps array is empty".into()));
    }

    // 4) 解析每个步骤
    let mut steps: Vec<PlanStep> = Vec::with_capacity(steps_value.len());
    for (idx, step_value) in steps_value.iter().enumerate() {
        let step: PlanStep = serde_json::from_value(step_value.clone()).map_err(|e| {
            LlmError::ParseError(format!("Failed to parse step #{}: {}", idx + 1, e))
        })?;

        // 校验 step_goal 非空
        if step.step_goal.trim().is_empty() {
            return Err(LlmError::ParseError(format!(
                "step #{}: step_goal must not be empty",
                idx + 1
            )));
        }

        // 校验 success_criteria：每条必须非空字符串
        for (c_idx, criterion) in step.success_criteria.iter().enumerate() {
            if criterion.trim().is_empty() {
                return Err(LlmError::ParseError(format!(
                    "step #{} (order={}): success_criteria[{}] must not be empty",
                    idx + 1, step.order, c_idx
                )));
            }
        }

        // 按 step_type 校验 actions
        match step.step_type {
            StepType::Deterministic => {
                // deterministic 必须给出至少 1 个 SubAction
                if step.actions.is_empty() {
                    return Err(LlmError::ParseError(format!(
                        "step #{} (order={}, deterministic): actions must not be empty; \
                         provide at least one SubAction with tool_name and parameters",
                        idx + 1, step.order
                    )));
                }
                // 校验每条 SubAction 的 tool_name / parameters 合法性
                for (a_idx, action) in step.actions.iter().enumerate() {
                    if action.tool_name.trim().is_empty() {
                        return Err(LlmError::ParseError(format!(
                            "step #{} action #{}: tool_name must not be empty",
                            idx + 1, a_idx + 1
                        )));
                    }
                    if !action.parameters.is_object() {
                        return Err(LlmError::ParseError(format!(
                            "step #{} action #{}: parameters must be a JSON object",
                            idx + 1, a_idx + 1
                        )));
                    }
                }
            }
            StepType::Reasoning => {
                // reasoning 步骤的 actions 必须为空（由模型推理生成）
                if !step.actions.is_empty() {
                    tracing::warn!(
                        "step #{} (order={}, reasoning) returned non-empty actions during \
                         planning; they will be ignored as reasoning steps use LLM inference only",
                        idx + 1, step.order
                    );
                }
            }
            StepType::Exploratory => {
                // exploratory 不应预设 actions（执行时由 ReAct 循环动态填充）
                if !step.actions.is_empty() {
                    tracing::warn!(
                        "step #{} (order={}, exploratory) returned non-empty actions during \
                         planning; they will be ignored and regenerated by ReAct at execution time",
                        idx + 1, step.order
                    );
                }
            }
        }

        steps.push(step);
    }

    // 5) 校验 order 连续递增
    for (idx, step) in steps.iter().enumerate() {
        let expected_order = (idx + 1) as u8;
        if step.order != expected_order {
            return Err(LlmError::ParseError(format!(
                "step order must be consecutive from 1; expected {}, got {}",
                expected_order, step.order
            )));
        }
    }

    // 6) 校验 depends_on（支持多依赖），每条都需合法
    for step in &steps {
        for &dep in &step.depends_on {
            if dep == 0 || dep >= step.order {
                return Err(LlmError::ParseError(format!(
                    "step #{} (order={}): depends_on={} is invalid (must be in 1..{})",
                    step.order, step.order, dep, step.order
                )));
            }
            // 去重提示（不阻断）
            if step.depends_on.iter().filter(|d| **d == dep).count() > 1 {
                tracing::warn!(
                    "step #{} (order={}): depends_on contains duplicate value {}",
                    step.order, step.order, dep
                );
            }
        }
    }

    // 7) 最后一个步骤的 success_criteria 必须非空（任务成功判定标准）
    if let Some(last) = steps.last() {
        if last.success_criteria.is_empty() {
            return Err(LlmError::ParseError(format!(
                "the last step (order={}) must have at least one success_criteria \
                 (objective condition(s) to determine task completion)",
                last.order
            )));
        }
    }

    Ok(PlansResponse { steps })
}

// ──────────────────────────────────────────────────────────────
// 单元测试
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // 反序列化兼容性测试：depends_on 只接受数组
    // =============================================================================

    #[test]
    fn test_parse_plans_response_basic() {
        // 两个 exploratory 步骤（actions 应为空），最后一个必须有 success_criteria
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "exploratory",
                    "step_goal": "打开百度首页",
                    "expected_output": "百度首页加载完成",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": [],
                    "actions": []
                },
                {
                    "order": 2,
                    "step_type": "exploratory",
                    "step_goal": "提取前三个搜索结果",
                    "depends_on": [1],
                    "input": ["{{step_1.output}}"],
                    "success_criteria": [
                        "返回内容是 Markdown 列表",
                        "列表包含 3 个搜索结果"
                    ],
                    "actions": []
                }
            ]
        }"#;

        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(resp.steps.len(), 2);
        assert_eq!(resp.steps[0].order, 1);
        assert_eq!(resp.steps[0].step_type, StepType::Exploratory);
        assert!(resp.steps[0].actions.is_empty());
        // depends_on = [1] → 反序列化为 vec![1]
        assert_eq!(resp.steps[1].depends_on, vec![1]);
        // input 字段透传
        assert_eq!(resp.steps[1].input, vec!["{{step_1.output}}".to_string()]);
        // success_criteria 透传
        assert_eq!(resp.steps[1].success_criteria.len(), 2);
    }

    #[test]
    fn test_parse_plans_response_deterministic_with_actions() {
        // 路径明确的 deterministic 步骤：必须给出完整的 actions，最后一步 success_criteria 非空
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "deterministic",
                    "step_goal": "打开百度首页",
                    "expected_output": "百度首页加载完成",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": [],
                    "actions": [
                        {
                            "order": 1,
                            "tool_name": "mcp__browser__navigate",
                            "parameters": {"url": "https://www.baidu.com"}
                        }
                    ]
                },
                {
                    "order": 2,
                    "step_type": "deterministic",
                    "step_goal": "截取当前页面",
                    "expected_output": "返回截图文件路径",
                    "depends_on": [1],
                    "input": [],
                    "success_criteria": [
                        "返回内容是本地文件路径",
                        "文件大小 > 0"
                    ],
                    "actions": [
                        {
                            "order": 1,
                            "tool_name": "mcp__browser__screenshot",
                            "parameters": {"path": "~/screenshots/baidu.png", "fullPage": true}
                        },
                        {
                            "order": 2,
                            "tool_name": "mcp__browser__wait",
                            "parameters": {"ms": 500}
                        }
                    ]
                }
            ]
        }"#;

        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(resp.steps.len(), 2);
        assert_eq!(resp.steps[0].step_type, StepType::Deterministic);
        assert_eq!(resp.steps[0].actions.len(), 1);
        assert_eq!(resp.steps[0].actions[0].tool_name, "mcp__browser__navigate");
        assert_eq!(
            resp.steps[0].actions[0].parameters.get("url").and_then(|v| v.as_str()),
            Some("https://www.baidu.com")
        );
        // 第二步多个 actions
        assert_eq!(resp.steps[1].actions.len(), 2);
        assert_eq!(resp.steps[1].actions[1].tool_name, "mcp__browser__wait");
        // depends_on = [1]（数组）→ 保持 vec![1]
        assert_eq!(resp.steps[1].depends_on, vec![1]);
    }

    #[test]
    fn test_parse_plans_response_mixed_scenario() {
        // 混合场景：deterministic（带 actions）+ exploratory（空 actions），最后一步必填 success_criteria
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "deterministic",
                    "step_goal": "打开邮箱登录页",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": [],
                    "actions": [
                        {
                            "order": 1,
                            "tool_name": "mcp__browser__navigate",
                            "parameters": {"url": "https://mail.example.com/login"}
                        }
                    ]
                },
                {
                    "order": 2,
                    "step_type": "exploratory",
                    "step_goal": "填写账号密码并登录",
                    "depends_on": [1],
                    "input": [],
                    "success_criteria": [],
                    "actions": []
                },
                {
                    "order": 3,
                    "step_type": "exploratory",
                    "step_goal": "下载最新附件",
                    "depends_on": [2],
                    "input": [],
                    "success_criteria": [
                        "附件已保存到本地",
                        "返回本地文件绝对路径"
                    ],
                    "actions": []
                }
            ]
        }"#;

        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(resp.steps.len(), 3);
        assert_eq!(resp.steps[0].step_type, StepType::Deterministic);
        assert_eq!(resp.steps[0].actions.len(), 1);
        assert_eq!(resp.steps[1].step_type, StepType::Exploratory);
        assert!(resp.steps[1].actions.is_empty());
        assert_eq!(resp.steps[2].step_type, StepType::Exploratory);
        assert!(resp.steps[2].actions.is_empty());
        assert_eq!(resp.steps[2].success_criteria.len(), 2);
    }

    // =============================================================================
    // 校验失败测试
    // =============================================================================

    #[test]
    fn test_parse_plans_response_deterministic_empty_actions_fails() {
        // deterministic 步骤 actions 为空 → 应报错
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "deterministic",
                    "step_goal": "打开百度",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": ["done"],
                    "actions": []
                }
            ]
        }"#;
        let err = parse_plans_response(json).expect_err("should fail");
        assert!(err.to_string().contains("deterministic"), "error: {}", err);
    }

    #[test]
    fn test_parse_plans_response_invalid_subaction_tool_name() {
        // SubAction 的 tool_name 为空 → 应报错
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "deterministic",
                    "step_goal": "test",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": ["done"],
                    "actions": [
                        {"order": 1, "tool_name": "", "parameters": {}}
                    ]
                }
            ]
        }"#;
        assert!(parse_plans_response(json).is_err());
    }

    #[test]
    fn test_parse_plans_response_invalid_subaction_parameters() {
        // SubAction 的 parameters 不是对象 → 应报错
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "deterministic",
                    "step_goal": "test",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": ["done"],
                    "actions": [
                        {"order": 1, "tool_name": "t", "parameters": "not-an-object"}
                    ]
                }
            ]
        }"#;
        assert!(parse_plans_response(json).is_err());
    }

    #[test]
    fn test_parse_plans_response_with_markdown() {
        // Markdown 包裹的 JSON 也能解析
        let response = r#"```json
{
  "steps": [
    {
      "order": 1,
      "step_goal": "test",
      "step_type": "deterministic",
      "depends_on": [],
      "input": [],
      "success_criteria": ["done"],
      "actions": [{"order": 1, "tool_name": "mcp__t", "parameters": {}}]
    }
  ]
}
```"#;
        let resp = parse_plans_response(response).expect("should parse");
        assert_eq!(resp.steps.len(), 1);
        assert_eq!(resp.steps[0].step_type, StepType::Deterministic);
        assert_eq!(resp.steps[0].actions.len(), 1);
    }

    #[test]
    fn test_parse_plans_response_invalid_order() {
        // order 必须从 1 开始连续递增，跳跃的 order 应报错
        let json = r#"{"steps": [{"order": 2, "step_goal": "skip", "step_type": "exploratory", "depends_on": [], "input": [], "success_criteria": ["done"], "actions": []}]}"#;
        assert!(parse_plans_response(json).is_err());
    }

    #[test]
    fn test_parse_plans_response_invalid_depends_on() {
        // depends_on 指向不存在的步骤 → 应报错
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": [], "success_criteria": [], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]},
                {"order": 2, "step_goal": "b", "depends_on": [5], "step_type": "exploratory", "input": [], "success_criteria": ["done"], "actions": []}
            ]
        }"#;
        assert!(parse_plans_response(json).is_err());
    }

    #[test]
    fn test_parse_plans_response_empty_steps() {
        let json = r#"{"steps": []}"#;
        assert!(parse_plans_response(json).is_err());
    }

    // =============================================================================
    // 新增：depends_on 多形式 + success_criteria 校验 + input 字段
    // =============================================================================

    #[test]
    fn test_parse_plans_response_depends_on_array_form() {
        // depends_on 直接使用数组
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": [], "success_criteria": [], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]},
                {"order": 2, "step_goal": "b", "depends_on": [1], "step_type": "exploratory", "input": [], "success_criteria": ["done"], "actions": []}
            ]
        }"#;
        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(resp.steps[1].depends_on, vec![1]);
    }

    #[test]
    fn test_parse_plans_response_depends_on_null_form() {
        // depends_on 空数组 → 结果为空 Vec
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": [], "success_criteria": ["done"], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]}
            ]
        }"#;
        let resp = parse_plans_response(json).expect("parse should succeed");
        assert!(resp.steps[0].depends_on.is_empty());
    }

    #[test]
    fn test_parse_plans_response_multi_dependency() {
        // 多依赖：步骤 3 同时依赖步骤 1 与步骤 2
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "fetch a", "depends_on": [], "input": [], "success_criteria": [], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]},
                {"order": 2, "step_goal": "fetch b", "depends_on": [], "input": [], "success_criteria": [], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]},
                {"order": 3, "step_goal": "merge", "depends_on": [1, 2], "step_type": "exploratory", "input": ["{{step_1.output}}", "{{step_2.output}}"], "success_criteria": ["merged"], "actions": []}
            ]
        }"#;
        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(resp.steps[2].depends_on, vec![1, 2]);
        assert_eq!(
            resp.steps[2].input,
            vec!["{{step_1.output}}".to_string(), "{{step_2.output}}".to_string()]
        );
    }

    #[test]
    fn test_parse_plans_response_multi_dependency_any_invalid_fails() {
        // 多依赖中只要有一条非法就报错
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": [], "success_criteria": [], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]},
                {"order": 2, "step_goal": "b", "depends_on": [1, 9], "step_type": "exploratory", "input": [], "success_criteria": ["done"], "actions": []}
            ]
        }"#;
        assert!(parse_plans_response(json).is_err());
    }

    #[test]
    fn test_parse_plans_response_last_step_requires_success_criteria() {
        // 最后一个步骤 success_criteria 为空 → 应报错
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": [], "success_criteria": [], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]},
                {"order": 2, "step_goal": "b", "depends_on": [1], "step_type": "exploratory", "input": [], "success_criteria": [], "actions": []}
            ]
        }"#;
        let err = parse_plans_response(json).expect_err("should fail");
        assert!(
            err.to_string().contains("success_criteria"),
            "error: {}",
            err
        );
    }

    #[test]
    fn test_parse_plans_response_empty_success_criteria_string_fails() {
        // success_criteria 中有空字符串 → 应报错
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": [], "success_criteria": ["valid", ""], "actions": []}
            ]
        }"#;
        let err = parse_plans_response(json).expect_err("should fail");
        assert!(
            err.to_string().contains("success_criteria[1]"),
            "error: {}",
            err
        );
    }

    #[test]
    fn test_parse_plans_response_with_input_field() {
        // input 字段正确透传
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": ["user.id", "config.timeout"], "success_criteria": [], "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]},
                {"order": 2, "step_goal": "b", "depends_on": [1], "step_type": "exploratory", "input": ["{{step_1.output}}"], "success_criteria": ["done"], "actions": []}
            ]
        }"#;
        let resp = parse_plans_response(json).expect("parse should succeed");
        assert_eq!(
            resp.steps[0].input,
            vec!["user.id".to_string(), "config.timeout".to_string()]
        );
        assert_eq!(resp.steps[1].input, vec!["{{step_1.output}}".to_string()]);
    }

    #[test]
    fn test_parse_plans_response_exploratory_with_actions_warns() {
        // exploratory 步骤返回非空 actions 不报错，仅警告（不影响解析）
        let json = r#"{
            "steps": [
                {"order": 1, "step_goal": "a", "depends_on": [], "input": [], "success_criteria": ["done"], "step_type": "exploratory", "actions": [{"order": 1, "tool_name": "t", "parameters": {}}]}
            ]
        }"#;
        let resp = parse_plans_response(json).expect("parse should succeed");
        assert!(!resp.steps[0].actions.is_empty());
    }

    // =============================================================================
    // Builder / 构造器测试
    // =============================================================================

    #[test]
    fn test_plan_step_exploratory_constructor() {
        // exploratory 构造器 + with_dependency（push 语义）+ with_input + with_success_criteria
        let step = PlanStep::exploratory(2, "test goal")
            .with_expected_output("test output")
            .with_dependency(1)
            .with_input(["{{step_1.output}}"])
            .with_success_criteria(["done"]);
        assert_eq!(step.order, 2);
        assert_eq!(step.step_type, StepType::Exploratory);
        assert!(step.actions.is_empty());
        // with_dependency 是 push 语义
        assert_eq!(step.depends_on, vec![1]);
        assert_eq!(step.expected_output.as_deref(), Some("test output"));
        assert_eq!(step.input, vec!["{{step_1.output}}".to_string()]);
        assert_eq!(step.success_criteria, vec!["done".to_string()]);
    }

    #[test]
    fn test_plan_step_with_dependency_is_push_semantics() {
        // 多次调用 with_dependency 应累加到 Vec
        let step = PlanStep::exploratory(3, "merge")
            .with_dependency(1)
            .with_dependency(2);
        assert_eq!(step.depends_on, vec![1, 2]);
    }

    #[test]
    fn test_plan_step_with_dependencies_batch() {
        // with_dependencies 一次性接受多个
        let step = PlanStep::exploratory(3, "merge").with_dependencies([1, 2]);
        assert_eq!(step.depends_on, vec![1, 2]);

        // 与 with_dependency 链式混合
        let step2 = PlanStep::exploratory(4, "merge2")
            .with_dependencies([1, 2])
            .with_dependency(3);
        assert_eq!(step2.depends_on, vec![1, 2, 3]);
    }

    #[test]
    fn test_plan_step_with_input_builder() {
        let step = PlanStep::exploratory(2, "use data").with_input(["step_1.user_id", "step_1.config"]);
        assert_eq!(
            step.input,
            vec!["step_1.user_id".to_string(), "step_1.config".to_string()]
        );
    }

    #[test]
    fn test_plan_step_with_success_criteria_builder() {
        let step = PlanStep::exploratory(2, "final")
            .with_success_criteria(["c1", "c2", "c3"]);
        assert_eq!(
            step.success_criteria,
            vec!["c1".to_string(), "c2".to_string(), "c3".to_string()]
        );
    }

    #[test]
    fn test_plan_step_deterministic_constructor_preserves_actions() {
        // PlanStep::new 是确定性步骤构造器，自动把 tool_name 转成第一个 SubAction
        let step = PlanStep::new(1, "mcp__browser__navigate", "打开百度首页")
            .with_expected_output("百度首页加载完成");
        assert_eq!(step.order, 1);
        assert_eq!(step.step_type, StepType::Deterministic);
        assert_eq!(step.actions.len(), 1);
        assert_eq!(step.actions[0].tool_name, "mcp__browser__navigate");
        assert_eq!(step.actions[0].order, 1);
        assert_eq!(step.expected_output.as_deref(), Some("百度首页加载完成"));
        // 构造器默认 Vec 都为空
        assert!(step.depends_on.is_empty());
        assert!(step.input.is_empty());
        assert!(step.success_criteria.is_empty());
    }

    // =============================================================================
    // build_plans_user_message 新签名测试
    // =============================================================================

    #[test]
    fn test_build_plans_user_message_new_signature() {
        // 新签名：单参透传
        let content = "【用户请求】\n查询天气\n\n【意图分析 reasoning】\n调用 API";
        let msg = build_plans_user_message(content);
        assert_eq!(msg, content);
    }

    // =============================================================================
    // 序列化兼容性测试
    // =============================================================================

    #[test]
    fn test_plan_step_serialize_depends_on_as_array() {
        // 序列化时 depends_on 永远是数组
        let step = PlanStep::exploratory(2, "use").with_dependency(1);
        let s = serde_json::to_string(&step).unwrap();
        assert!(s.contains("\"depends_on\":[1]"), "serialized: {}", s);
    }

    #[test]
    fn test_plan_step_serialize_empty_depends_on_as_empty_array() {
        // 空依赖序列化为 [] 而非 null
        let step = PlanStep::exploratory(1, "fresh");
        let s = serde_json::to_string(&step).unwrap();
        assert!(s.contains("\"depends_on\":[]"), "serialized: {}", s);
    }
}
