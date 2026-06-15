# PlanExecutor - 计划执行器

## 概述

PlanExecutor 负责执行由 IntentAnalyzer 生成的多步骤计划（IntentPlan）。支持两种执行模式：

- **Deterministic 模式**：确定性步骤，工具和参数在规划时已知
- **Exploratory 模式**：探索性步骤，需要在执行时动态决定操作

## ReAct 模式（自主决策）

Exploratory 步骤采用 **ReAct（Reasoning + Acting）模式**，让 LLM 在执行时自主决策下一步操作：

```
IntentPlan ──▶ 执行 Step
                    │
                    ▼  边观察、边执行
              ┌─────────────────────────┐
              │ "当前页面有搜索框吗？"  │
              │   观察 → 决策 → 执行   │
              │   失败 → 重试 → 调整   │
              └─────────────────────────┘
```

### Agent 循环流程

1. **LLM 决策**：根据当前状态，决定下一步使用什么工具
2. **执行工具**：调用 MCP 工具执行操作
3. **验证目标**：LLM 判断是否达成步骤目标
4. **循环迭代**：未达成则继续，直到达成或达到最大调用次数

### 核心优势

| 特性 | 说明 |
|------|------|
| **自主决策** | LLM 根据实际页面状态动态选择工具 |
| **自适应** | 页面结构变化时自动调整执行策略 |
| **避免幻觉** | 不预先猜测 CSS 选择器，执行时可见即可用 |
| **可追溯** | 每步操作都记录到消息历史，供后续步骤参考 |

### 状态管理

- `MessageContext`：管理 LLM 对话消息历史
- `tool_calls_history`：记录所有工具调用及结果
- `all_outputs`：收集所有步骤输出作为最终返回

### 目标验证

每次工具执行后，LLM 会判断是否达成 `step_goal`。验证结果：
- `achieved = true`：目标达成，结束当前步骤
- `achieved = false`：目标未达成，继续 Agent 循环

## 架构

```
┌─────────────────┐
│   IntentPlan    │ 高层计划（IntentAnalyzer 生成）
│                 │
│  Step 1: ...    │
│  Step 2: ...    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  PlanExecutor   │ 执行引擎
│                 │
│  Deterministic  │ 直接执行已知工具
│  Exploratory    │ ReAct 循环自主决策
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  ToolExecutor   │ 工具执行（MCP）
└─────────────────┘
```

## 相关文件

- `plan_executor.rs` - 执行器主逻辑
- `exploratory_step.rs` - 探索性步骤（ReAct 循环）
- `deterministic_step.rs` - 确定性步骤执行
- `message_context.rs` - 消息上下文管理