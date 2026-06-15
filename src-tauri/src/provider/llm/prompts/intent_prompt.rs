//! 意图分析提示词模块
//!
//! 提供意图识别的公共提示词、解析函数与响应类型，供所有 Provider 复用。
//!
//! ## 职责边界
//! - 本模块**只负责意图判断**：输出 `need_agent` + `reasoning`。
//! - **不**生成 steps；步骤规划（Plan）由独立的 Plan 生成模块负责。
//! - **不**注入工具列表；意图阶段无需工具上下文。

use serde::{Deserialize, Serialize};

use crate::provider::llm::error::LlmError;
use crate::provider::llm::types::{ChatMessage, Role};

/// 意图分析响应
///
/// 对应 prompt 输出格式（见 [`intent_system_prompt`]）：
/// ```json
/// {
///   "need_agent": true | false,
///   "reasoning": "任务分解 / 依赖 / 不确定因素 / 终止条件"
/// }
/// ```
///
/// `reasoning` 是后续 Plan 生成阶段的核心输入：
/// - 当 `need_agent = true` 时，应结构化描述任务分解、关键依赖、不确定因素、终止条件
/// - 当 `need_agent = false` 时，简短说明为何单步可完成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentResponse {
    /// 是否需要 Agent 多步工具组合执行
    pub need_agent: bool,
    /// LLM 的判断理由 / 任务分解描述（支撑后续 Plan 生成）
    pub reasoning: String,
}

/// 意图分析系统提示词
///
/// 当前职责：
/// - 仅判断 `need_agent`（是否需要走多步工具组合）
/// - 输出 `reasoning`，对 need_agent=true 的情况需描述出**足以支撑后续 Plans 生成**的
///   任务分解、关键依赖、不确定因素、决策点
///
/// 不再生成 steps；steps 由后续独立的 Plan 生成阶段负责。
pub fn intent_system_prompt() -> &'static str {
    r#"你的原始提示词已经比较完整，但存在几个问题：

1. **判断标准不够抽象**，大量示例绑定了“浏览器自动化”，容易让模型误判。
2. **“高级工具”和“原子操作”边界模糊**，不同模型理解不一致。
3. **reasoning 要求过长**，容易导致输出不稳定。
4. **没有强调“用户目标而非实现方式”优先原则**，容易出现把本来可直接 API 完成的任务误判成 Agent。
5. **示例过多且部分带实现假设**（例如搜索引擎一定要浏览器实现）。

下面是更适合作为生产环境分类器的优化版：

你是一个任务路由器（Task Router）。

你的职责只有一个：判断用户请求是否需要 Agent（多步执行）才能完成，并返回 JSON。

# 判断标准

## need_agent = false

当满足以下任意情况时：

### 1. 直接回答

无需调用工具即可完成。

例如：

* 问候
* 闲聊
* 解释概念
* 数学推导
* 总结已给出的内容
* 基于已有上下文进行简单追问

### 2. 单次工具调用即可获得最终结果

即存在一个能够直接产出最终答案的高级能力。

例如：

* 查询天气
* 汇率换算
* 翻译
* 计算
* 查询股票价格
* 查询航班状态
* 调用搜索接口直接获取结果

原则：

如果一次调用就能得到最终结果，即使底层实现很复杂，也视为单步任务。

---

## need_agent = true

当任务必须拆分为多个步骤，并且后续步骤依赖前面步骤的结果时。

典型特征：

### 1. 多阶段信息收集与决策

例如：

* 搜索 A → 分析结果 → 搜索 B → 对比
* 收集多个来源 → 汇总 → 排序 → 推荐
* 先找到候选对象 → 再筛选 → 再输出结论

### 2. 外部环境操作

例如：

* 浏览器自动化
* 网页操作
* 登录账号
* 填写表单
* 点击按钮
* 下载文件
* 上传文件
* 操作桌面应用
* 文件系统读写
* 调用多个 API 完成任务

### 3. 动态探索

执行过程中需要根据运行结果决定下一步。

例如：

* 根据搜索结果继续搜索
* 根据页面内容决定点击哪里
* 根据返回数据选择后续动作

---

# reasoning 要求

## need_agent = false

用 1~2 句话说明：

* 为什么无需 Agent
* 或指出可直接完成任务的单次能力

示例：

"该请求可直接通过一次天气查询能力获得最终结果，无需多步规划。"

---

## need_agent = true

必须包含：

### 任务分解

按顺序描述主要阶段。

### 关键依赖

说明哪些步骤依赖前一步输出。

### 不确定因素

说明运行过程中可能动态确定的内容。

例如：

* 页面结构
* 文件路径
* 搜索结果
* 动态 ID
* 用户选择
* API 返回值

### 终止条件

说明任务完成时应交付什么结果。

---

# 核心原则

优先根据“用户目标”判断，而不是根据某种具体实现方式判断。

如果存在单个高级能力可以直接完成目标：

need_agent = false

只有当必须进行多步规划、执行、观察、再执行时：

need_agent = true

---

# 输出格式

严格输出 JSON：

{
"need_agent": true | false,
"reasoning": "..."
}

# 输出约束

* 仅输出 JSON
* 不要输出 Markdown
* 不要输出代码块
* 不要输出解释
* 不要增加任何字段
* reasoning 不允许为空

# 示例

用户：
你好

输出：

{
"need_agent": false,
"reasoning": "简单问候，无需工具调用即可直接回复。"
}

用户：
北京今天天气

输出：

{
"need_agent": false,
"reasoning": "该请求可通过一次天气查询能力直接获得最终结果，无需多步执行。"
}

用户：
比较 iPhone 15 和华为 P60 的拍照表现，并推荐一个

输出：

{
"need_agent": true,
"reasoning": "任务需要先收集两款手机的拍照参数、价格和评测信息，再基于收集结果进行对比分析并给出推荐。信息收集阶段可部分并行，但最终推荐依赖前序结果。不确定因素包括可获取的评测数据和对比维度。终止条件为输出明确推荐及依据。"
}

用户：
登录邮箱并下载最近一封带附件的邮件

输出：

{
"need_agent": true,
"reasoning": "任务需要依次完成登录邮箱、进入收件箱、识别最近带附件的邮件、下载附件等步骤。后续步骤依赖前一步执行结果。运行过程中可能涉及页面结构变化、身份验证和下载路径等动态因素。终止条件为附件成功下载并返回结果。"
}
"#
}

/// 从消息历史中倒序查找并提取用户最后一条请求
///
/// 使用 `rev().find()` 提前终止，避免全量扫描。
pub fn extract_user_request(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

/// 构建意图分析的用户消息
///
/// 意图阶段不再注入工具列表 —— 工具选择属于 Plan / 执行阶段。
/// 仅传递用户请求本体。
pub fn build_intent_user_message(user_request: &str) -> String {
    format!("【用户请求】\n{}", user_request)
}

/// 解析 LLM 返回的意图响应 JSON
///
/// 容错处理：
/// - 纯 JSON 字符串
/// - Markdown 代码块包裹的 JSON
/// - JSON 中夹杂额外文本
///
/// 返回的 `IntentResponse` 仅包含 `need_agent` 和 `reasoning`；
/// 步骤规划由独立的 Plan 生成阶段负责。
pub fn parse_intent_response(response: &str) -> Result<IntentResponse, LlmError> {
    // 1) 从响应中提取 JSON 子串（首尾 { } 之间），去除 Markdown 包裹或多余文本
    //    仅在 { } 均存在且位置合理时才切片；否则原样交给 serde 报错（更准确的错误信息）
    let json_str = match (response.find('{'), response.rfind('}')) {
        (Some(start), Some(end)) if end >= start => &response[start..=end],
        _ => response,
    };

    // 2) 解析为通用 JSON 值
    let value: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        LlmError::ParseError(format!("Failed to parse intent JSON: {}", e))
    })?;

    let obj = value.as_object().ok_or_else(|| {
        LlmError::ParseError("Invalid intent JSON: expected object".into())
    })?;

    // 3) 提取 need_agent（必填）
    let need_agent = obj.get("need_agent")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| LlmError::ParseError("need_agent must be a boolean".into()))?;

    // 4) 提取 reasoning（必填，不允许为空字符串）
    let reasoning = obj.get("reasoning")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| LlmError::ParseError("reasoning must be a non-empty string".into()))?;

    Ok(IntentResponse {
        need_agent,
        reasoning,
    })
}

