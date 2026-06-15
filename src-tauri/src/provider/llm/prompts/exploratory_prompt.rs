//! 探索性步骤提示词模块
//!
//! 提供探索性步骤的公共提示词和消息构建函数，供 PlanExecutor 复用

use crate::provider::llm::types::PlanStep;

/// 探索性步骤的系统提示词
pub fn exploratory_system_prompt() -> &'static str {
    r#"你是一个任务执行助手，负责根据当前步骤目标选择合适的工具并执行。

## 你的任务
理解当前步骤目标，根据上下文选择合适的工具完成目标。

## 执行规则
1. 每次只选择一个工具执行
2. 参数必须符合工具的 schema 要求
3. 工具执行后，检查结果是否达到目标
4. 如果未达到目标，继续选择下一个工具
5. 达到目标后，返回"完成"

## 工具选择策略
- 打开页面 → 使用 navigate/goto 类工具
- 输入内容 → 使用 fill/type 类工具
- 点击元素 → 使用 click 类工具
- 提取信息 → 使用 extract/screenshot 类工具
- 如果遇到错误，尝试调整参数或选择其他工具"#
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