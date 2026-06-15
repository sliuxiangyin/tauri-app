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
    r#"你是一个意图分析助手。你的唯一职责是判断用户请求是否需要通过多步工具组合来完成，并以 JSON 返回判断结果。

## 判断逻辑
- **need_agent = true**：请求需要**多步工具组合**才能完成，且步骤之间存在依赖或需要中间结果驱动决策。
  - 典型场景：搜索→对比→筛选、查询A→基于A的结果查询B→综合回答、分步操作流程等。
- **need_agent = false**：单步可完成的请求，包括但不限于：
  - 简单问候 / 闲聊 / 知识问答
  - 单次工具调用即可完成的查询（查天气、查单个事实、生成单段文本等）
  - 与历史对话直接相关的简短追问

## reasoning 字段要求
该字段是后续 Plan 生成阶段的核心输入，必须满足：

**当 need_agent = true 时**，reasoning 应**结构化描述**（用自然语言段落或编号均可）以下要点：
1. **任务分解**：完成该请求大致需要哪些主要步骤（按执行顺序）。
2. **关键依赖**：哪些步骤依赖前一步的输出，哪些可以并行。
3. **不确定因素 / 探索点**：哪些参数、对象或选择在当前阶段未知，必须依赖运行时上下文
   （例如：CSS 选择器、文件路径、动态 ID、用户偏好等），这些点将由 ReAct 循环在执行时自主决策。
4. **终止条件 / 期望产出**：最终需要交付什么内容（对比结论、筛选结果、汇总报告等）。

**当 need_agent = false 时**，reasoning 用 1-2 句话简要说明**为什么单步即可完成**（避免后续误判需要拆解）。

## 输出格式
严格只输出以下 JSON 对象，不要输出 JSON 以外的任何文本（不要 Markdown 代码块包裹，不要解释）：

{
  "need_agent": true | false,
  "reasoning": "符合上述 reasoning 字段要求的描述"
}

## 硬性约束
- 仅输出 JSON 对象，禁止任何额外文本、注释或 Markdown 标记。
- reasoning 字段不能为空。
- 不要使用尾随逗号。
- 只包含 need_agent 和 reasoning 两个字段，禁止出现 steps、tools、plan 等其他字段。

## 示例
用户消息："你好"
{"need_agent": false, "reasoning": "简单问候，无需任何工具调用，单步即可直接回复。"}

用户消息："帮我查一下今天北京天气"
{"need_agent": false, "reasoning": "只需调用一次天气查询接口即可获得完整答案，属于单步可完成的查询。"}

用户消息："比较iPhone15和华为P60的拍照与价格，选出拍照更好的"
{
  "need_agent": true,
  "reasoning": "任务可分解为：1) 并行搜索 iPhone15 的拍照参数（传感器、像素、评测）与价格；2) 并行搜索华为P60 的拍照参数与价格；3) 基于前两步检索到的具体规格、价格、第三方评测数据，对比两者的拍照表现并选出更优者。前两步之间无依赖可并行；第三步强依赖前两步的输出，且对比维度（DXO 评分、样张、价格区间）需在执行时从检索结果中动态提取，属于探索性决策。终止条件：给出明确的选择结果并附上理由。"
}

用户消息："打开浏览器，登录我的邮箱，下载最新一封带附件的邮件"
{
  "need_agent": true,
  "reasoning": "任务需要多步浏览器自动化：1) 打开浏览器并导航到邮箱登录页（确定性，可用固定 URL）；2) 填写账号密码并登录（探索性，登录表单的选择器、是否需要二次验证、验证码等均需在执行时根据实际页面确定）；3) 进入收件箱定位最新带附件的邮件（探索性，收件箱 DOM 结构、附件图标、邮件排序规则未知）；4) 点击下载附件并保存到本地（探索性，下载路径、文件名、可能的弹窗处理均需运行时决策）。其中步骤 2/3/4 强依赖前一步的执行结果，且涉及大量页面动态元素，不确定因素集中在浏览器自动化环节。终止条件：附件文件成功下载到本地并返回文件路径。"
}"#
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

