//! 探索性步骤提示词模块
//!
//! 提供探索性步骤的公共提示词和消息构建函数，供 PlanExecutor 复用

use crate::provider::llm::types::PlanStep;

/// ReAct Agent Prompt 
pub fn re_act_system_prompt() -> &'static str {
    r#"You are an autonomous execution agent.

Your responsibility is to execute tasks by following a dynamic execution plan.

You are NOT a simple ReAct agent.

You operate using:

Goal → Plan → Execute → Observe → Replan → Complete

---

# Mission

Given a user request:

1. Understand the objective.
2. Identify dependencies.
3. Determine uncertainty.
4. Build an internal execution plan.
5. Execute only the next justified step.
6. Continuously update the plan based on observations.
7. Stop immediately once the objective has been achieved.

---

# Internal Plan Model

Represent the task internally as a set of steps.

Each step should contain:

* step_goal
* expected_output
* depends_on
* success_criteria

Do NOT expose this plan unless explicitly requested.

---

# Step Classification

Every step belongs to one of three categories:

## deterministic

Use when:

* Required tool is known
* Parameters are known
* Execution path is known

Examples:

* Read file
* Query database
* Call API
* Download URL

---

## reasoning

Use when:

* Summarization
* Classification
* Comparison
* Information extraction
* Report generation
* Decision making

Reasoning steps do not require tools unless external information is missing.

---

## exploratory

Use when:

* Environment is uncertain
* UI structure unknown
* Login flow unknown
* Dynamic identifiers required
* Runtime discovery required

Examples:

* Website navigation
* Browser automation
* Dynamic page interaction
* Searching for unknown objects

---

# Execution Rules

Before executing any tool:

1. Verify current step dependencies are satisfied.
2. Verify required inputs exist.
3. Verify execution contributes to task completion.

Never execute actions that are not justified by the current plan state.

---

# Observation Rules

After every tool result:

1. Evaluate whether the current step succeeded.
2. Update known information.
3. Reassess remaining steps.
4. Decide whether:

* Continue current step
* Move to next step
* Create a new exploratory step
* Finish task

Observations always override assumptions.

Tool outputs are the source of truth.

---

# Replanning Rules

You may modify the internal plan when:

* New information appears
* Assumptions become invalid
* Tool outputs reveal better paths

You must NOT blindly follow an outdated plan.

The plan is adaptive.

---

# Tool Usage Rules

* Prefer the minimum number of tool calls.
* Avoid redundant actions.
* Never repeat identical calls without reason.
* Never fabricate tool results.
* Never ignore observations.

Every tool call must have a clear objective.

---

# Failure Handling

If execution fails:

1. Determine whether failure is temporary.
2. Retry only when justified.
3. Consider alternative approaches.
4. Update the plan.
5. Avoid infinite retry loops.

If the task cannot proceed:

Explain what dependency is missing.

---

# Completion Rules

Stop execution immediately when:

* User objective is satisfied.
* Required deliverable has been produced.
* Additional actions provide no meaningful benefit.

Do not continue exploring after completion criteria are met.

---

# Final Response Rules

When task is complete:

* Provide the final deliverable.
* Include supporting evidence when necessary.
* Do not expose internal reasoning.
* Do not expose hidden planning process.
* Do not output Thought / Action / Observation traces.
"#
}

/// 判断目标是否达成的系统提示词
pub fn goal_check_system_prompt() -> &'static str {
    r#"你是一个任务评估助手，负责判断当前步骤目标是否已达成。

## 你的任务
根据执行历史，判断步骤目标是否已达成。

## 判断标准
- 目标已达成：返回 YES
- 目标未达成：返回 NO，并说明原因

## 输出格式
```
判断: YES/NO
原因: <简要说明>
```"#
}

/// 构建探索性步骤的初始消息
pub fn build_exploratory_initial_message(
    step: &PlanStep,
    history_summary: Option<&str>,
) -> String {
    let mut msg = String::new();

    // 添加步骤目标
    msg.push_str(&format!("【当前步骤目标】\n{}\n\n", step.step_goal));

    // 添加期望输出（如果有）
    if let Some(expected) = &step.expected_output {
        msg.push_str(&format!("【期望输出】\n{}\n\n", expected));
    }

    // 添加已执行步骤历史
    if let Some(history) = history_summary {
        if !history.is_empty() {
            msg.push_str(&format!("【已执行步骤】\n{}\n\n", history));
        }
    }

    msg.push_str("请选择合适的工具开始执行。");
    tracing::debug!("Exploratory initial message: {}", msg);
    msg
}

/// 从已执行步骤构建历史摘要
pub fn build_history_summary(executed_steps: &[PlanStep]) -> String {
    if executed_steps.is_empty() {
        return String::new()
    }

    executed_steps
        .iter()
        .filter_map(|s| {
            // 从 actions 列表获取输出和工具名
            let output = s
                .actions
                .last()
                .and_then(|a| a.output.as_ref())
                .map(|o| o.as_str())
                .unwrap_or("（无输出）");

            let tool_name = s
                .actions
                .first()
                .map(|a| a.tool_name.as_str())
                .unwrap_or("（无工具）");

            Some(format!(
                "步骤{} ({}): {} - 输出: {}",
                s.order,
                tool_name,
                s.step_goal,
                output
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 构建工具执行后的检查消息
pub fn build_goal_check_message(
    step_goal: &str,
    tool_calls_history: &[(String, String)],
) -> String {
    let mut msg = String::new();

    msg.push_str(&format!("【当前步骤目标】\n{}\n\n", step_goal));

    msg.push_str("【工具执行历史】\n");
    for (tool_name, result) in tool_calls_history {
        msg.push_str(&format!("- {}: {}\n", tool_name, result));
    }

    msg.push_str("\n请判断目标是否已达成。");

    msg
}

/// 解析目标判断结果
pub fn parse_goal_check_response(response: &str) -> (bool, String) {
    let response = response.trim();
    
    let mut achieved = false;
    let mut reason = String::new();

    for line in response.lines() {
        let line = line.trim();
        if line.starts_with("判断:") {
            let value = line.trim_start_matches("判断:").trim();
            achieved = value.to_uppercase().starts_with('Y');
        } else if line.starts_with("原因:") {
            reason = line.trim_start_matches("原因:").trim().to_string();
        }
    }

    if reason.is_empty() {
        reason = if achieved { "目标达成" } else { "目标未达成" }.to_string();
    }

    (achieved, reason)
}

// =============================================================================
// Replan 提示词
// =============================================================================

/// Replan 系统提示词
///
/// 用于步骤失败或观察发现新信息时，调用 LLM 重新规划剩余步骤。
pub fn replan_system_prompt() -> &'static str {
    r#"你是一个计划规划助手，负责根据当前执行情况重新规划剩余步骤。

## 你的任务
根据已执行的步骤历史和当前观察，修改剩余的执行计划。

## 你可以：
- 删除不再需要的步骤
- 新增必要的步骤
- 修改现有步骤的目标或参数
- 调整步骤的执行顺序
- 保持部分步骤不变

## 输出格式
输出一个 JSON 数组，包含修改后的步骤列表。每个步骤包含：
```json
[
  {
    "order": 1,
    "step_type": "deterministic" | "reasoning" | "exploratory",
    "step_goal": "步骤目标描述",
    "expected_output": "期望产出（可选）",
    "depends_on": [],
    "success_criteria": ["成功标准"]
  }
]
```

## 规则
- order 必须从 1 开始连续递增
- step_type 只能是 deterministic、reasoning、exploratory 之一
- depends_on 只能引用当前列表中前面的步骤的 order
- 保持计划简洁，不要添加不必要的步骤
- 只输出 JSON，不要输出其他内容
"#
}

/// 构建 Replan 的用户消息
pub fn build_replan_message(
    remaining_steps: &[PlanStep],
    observation_summary: &str,
    history_summary: &str,
) -> String {
    let mut msg = String::new();

    msg.push_str("【已执行步骤历史】\n");
    if history_summary.is_empty() {
        msg.push_str("（无）\n\n");
    } else {
        msg.push_str(history_summary);
        msg.push_str("\n\n");
    }

    msg.push_str(&format!("【最新观察】\n{}\n\n", observation_summary));

    msg.push_str("【当前剩余步骤】\n");
    if remaining_steps.is_empty() {
        msg.push_str("（无剩余步骤）\n\n");
    } else {
        for step in remaining_steps {
            msg.push_str(&format!(
                "步骤{} ({}): {} | 依赖: {:?}\n",
                step.order,
                serde_json::to_string(&step.step_type).unwrap_or_default(),
                step.step_goal,
                step.depends_on,
            ));
        }
        msg.push('\n');
    }

    msg.push_str("请根据以上信息重新规划剩余步骤。只输出 JSON 数组。");
    msg
}

/// 解析 Replan 响应（JSON 数组 -> Vec<PlanStep>）
pub fn parse_replan_response(response: &str) -> Result<Vec<PlanStep>, String> {
    let response = response.trim();

    // 尝试提取 JSON 数组（LLM 有时会包裹在 ```json ... ``` 中）
    let json_str = if response.starts_with("```") {
        response
            .lines()
            .filter(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        response.to_string()
    };

    serde_json::from_str::<Vec<PlanStep>>(&json_str)
        .map_err(|e| format!("Failed to parse replan response: {}", e))
}

// =============================================================================
// Reasoning 步骤提示词
// =============================================================================

/// Reasoning 步骤系统提示词
///
/// 用于推理性步骤（Summarization、Classification、Report generation 等）
pub fn reasoning_system_prompt() -> &'static str {
    r#"你是一个推理助手，负责根据提供的上下文信息完成推理任务。

## 你的任务
根据给定的步骤目标和上下文信息，生成推理结果。

## 支持的推理类型
- 摘要：将长文本浓缩为关键信息
- 分类：对内容进行类别判定
- 对比：比较多个信息的异同
- 信息提取：从非结构化文本中提取结构化数据
- 报告生成：综合多源信息生成报告
- 决策：根据条件做出判断

## 规则
- 只输出推理结果，不要输出推理过程
- 结果应简洁、准确、结构化
- 如果上下文信息不足，明确指出缺失的信息
"#
}

/// 构建 Reasoning 步骤的用户消息
pub fn build_reasoning_message(step_goal: &str, context_inputs: &str) -> String {
    let mut msg = String::new();

    msg.push_str(&format!("【推理目标】\n{}\n\n", step_goal));

    if !context_inputs.is_empty() {
        msg.push_str(&format!("【上下文信息】\n{}\n\n", context_inputs));
    }

    msg.push_str("请根据以上信息完成推理，直接输出结果。");
    msg
}