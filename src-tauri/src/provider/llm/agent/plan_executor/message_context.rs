//! 探索性步骤消息上下文管理
//!
//! 统一管理 Agent 循环中的 LLM 对话消息列表，
//! 封装消息的构建与追加操作。
//!
//! ## 消息前缀规范
//!
//! 所有步骤级消息统一使用 `步骤 {order} | {content}` 前缀，
//! 让 LLM 一眼识别消息所属步骤，避免混淆跨步骤记忆。
//!
//! - 步骤级消息：`步骤 2 | 目标: ...`、`步骤 2 | 工具 xxx 执行结果: ...`
//! - 计划级消息（跨步骤）：`计划 | 计划变更 原因: ...`
//! - System 消息：无前缀

use crate::provider::llm::types::{ChatMessage, Role};

/// 步骤级消息统一前缀
const STEP_PREFIX_FMT: &str = "步骤 {order} | ";

/// 计划级消息统一前缀（跨步骤）
const PLAN_PREFIX: &str = "计划 | ";

fn step_prefix(order: u8) -> String {
    STEP_PREFIX_FMT.replace("{order}", &order.to_string())
}

/// 探索性步骤的消息上下文
///
/// 封装 Agent 循环中的 LLM 对话消息列表，提供语义化方法管理消息生命周期。
pub(crate) struct MessageContext {
    /// LLM 对话消息列表
    messages: Vec<ChatMessage>,
}

impl MessageContext {
    /// 构造初始消息上下文
    ///
    /// 只初始化 System 消息，push_step 由调用方控制
    pub fn new(re_act_prompt: &str) -> Self {
        let messages = vec![ChatMessage::new(Role::System, re_act_prompt)];

        tracing::debug!(
            "MessageContext initialized with {} initial messages",
            messages.len()
        );

        Self { messages }
    }

    /// 获取当前消息列表（传给 LLM）
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// 添加工具执行结果到 LLM 消息历史
    ///
    /// 统一格式：`步骤 {order} | 工具 {name} 执行结果: {result}`
    pub fn push_tool_result(&mut self, step_order: u8, name: &str, result: &str) {
        self.messages.push(ChatMessage::new(
            Role::Tool,
            &format!(
                "{}工具 {} 执行结果: {}",
                step_prefix(step_order),
                name,
                result
            ),
        ));
    }

    /// 添加目标判断结果到 LLM 消息历史
    ///
    /// 统一格式：`步骤 {order} | 判断: {YES/NO} 原因: {reason}`
    pub fn push_goal_check(&mut self, step_order: u8, achieved: bool, reason: &str) {
        self.messages.push(ChatMessage::new(
            Role::Assistant,
            &format!(
                "{}判断: {}\n",
                step_prefix(step_order),
                if achieved { "YES" } else { "NO" },
            ),
        ));
        //  self.messages.push(ChatMessage::new(
        //     Role::Assistant,
        //     &format!(
        //         "{}判断: {}\n原因: {}",
        //         step_prefix(step_order),
        //         if achieved { "YES" } else { "NO" },
        //         reason
        //     ),
        // ));
    }

    /// 推入当前步骤目标（User 消息）
    ///
    /// 用于在 plan 级别共享的上下文中，标记"当前轮到哪个步骤"。
    /// 后续 LLM 决策时会把该消息一并送入 messages，让 LLM 知道本轮要做什么。
    ///
    /// 统一格式：`步骤 {order} | 目标: {goal}` + 可选 `期望产出: {output}`
    ///
    /// 注：本方法不会清空已有消息，**跨步骤累积**。如需重置，构造新的 `MessageContext`。
    pub fn push_step_goal(&mut self, order: u8, step_goal: &str, expected_output: Option<&str>) {
        let mut content = format!("{}目标: {}", step_prefix(order), step_goal);
        if let Some(out) = expected_output {
            content.push_str(&format!("\n期望产出: {}", out));
        }
        self.messages.push(ChatMessage::new(Role::User, content));
    }

    /// 记录 Observe 阶段的观察结论（Assistant 消息）
    ///
    /// 让 LLM 在后续决策中感知到之前步骤的执行结论和决策。
    ///
    /// 统一格式：`步骤 {order} | 【观察】结果: {成功/失败} 摘要: {summary}`
    pub fn push_observation(&mut self, step_order: u8, success: bool, summary: &str) {
        self.messages.push(ChatMessage::new(
            Role::Assistant,
            &format!(
                "{}【观察】结果: {}\n摘要: {}",
                step_prefix(step_order),
                if success { "成功" } else { "失败" },
                summary
            ),
        ));
    }

    /// 记录 Replan 决策（Assistant 消息）— 计划级（跨步骤）
    ///
    /// 告知 LLM 当前计划已变更，包含变更原因和新步骤摘要。
    ///
    /// 统一格式：`计划 | 【计划变更】原因: {reason} 新计划摘要: {summary}`
    pub fn push_replan_decision(&mut self, reason: &str, new_steps_summary: &str) {
        self.messages.push(ChatMessage::new(
            Role::Assistant,
            &format!(
                "{}【计划变更】\n原因: {}\n新计划摘要:\n{}",
                PLAN_PREFIX, reason, new_steps_summary
            ),
        ));
    }

    /// 记录步骤被跳过（Assistant 消息）
    ///
    /// 告知 LLM 某步骤因失败被跳过，后续步骤不应依赖其输出。
    ///
    /// 统一格式：`步骤 {order} | 【已跳过】原因: {reason}`
    pub fn push_step_skipped(&mut self, step_order: u8, reason: &str) {
        self.messages.push(ChatMessage::new(
            Role::Assistant,
            &format!("{}【已跳过】原因: {}", step_prefix(step_order), reason),
        ));
    }

    /// 记录重试上下文（User 消息）
    ///
    /// 在 RetryCurrent 决策触发新一轮执行前调用，让 LLM 明确知道：
    /// 1. 这是重试（不是新一轮）
    /// 2. 上次为什么失败（reason）
    /// 3. 已尝试过哪些工具（避免重复）
    ///
    /// 统一格式：`步骤 {order} | 【重试信号】第 {attempt} 次尝试 ⚠️ ... 💡 ...`
    ///
    /// # 参数
    /// - `step_order`: 当前步骤序号
    /// - `attempt`: 当前是第几次尝试（从 2 开始，1 表示首次）
    /// - `last_reason`: 上次失败原因（来自 ObserveDecision 的 reason）
    /// - `tried_tools`: 上次尝试中已使用过的工具名列表
    /// - `hint`: 可选的策略建议（领域无关，如"考虑换不同策略"）
    pub fn push_retry_attempt(
        &mut self,
        step_order: u8,
        attempt: u8,
        last_reason: &str,
        tried_tools: &[String],
        hint: Option<&str>,
    ) {
        let tools_str = if tried_tools.is_empty() {
            "(无)".to_string()
        } else {
            tried_tools.join(", ")
        };
        let hint_str = hint.unwrap_or("请尝试不同的策略，避免重复已尝试的工具");
        let content = format!(
            "{}【重试信号】第 {} 次尝试\n\
             ⚠️ 上次未达成目标\n\
             📋 失败原因: {}\n\
             🔧 已尝试工具: {}\n\
             💡 建议: {}",
            step_prefix(step_order),
            attempt,
            last_reason,
            tools_str,
            hint_str
        );
        self.messages.push(ChatMessage::new(Role::User, content));
    }

    /// Dump 完整消息历史到日志
    ///
    /// 计划执行完毕后调用，便于事后回溯整个 LLM 会话上下文（包含系统提示、
    /// 步骤目标、工具结果、观察结论、重试信号等所有消息）。
    ///
    /// 输出格式：
    /// ```text
    /// ========== msg_ctx 消息历史 dump 开始 (共 N 条) ==========
    /// [000] [System] 你是...
    /// [001] [User] 步骤 1 | 目标: ...
    /// [002] [Tool] 步骤 1 | 工具 browser_navigate 执行结果: ...
    /// ...
    /// ========== msg_ctx 消息历史 dump 结束 ==========
    /// ```
    ///
    /// 使用 `==========` 包裹便于 grep 过滤：
    /// ```bash
    /// grep "========== msg_ctx 消息历史 dump 开始 ==========" app.log -A 50
    /// ```
    ///
    /// # 参数
    /// - `max_content_len`: 单条消息最大展示字符数（按 Unicode 字符计算），超出部分截断。
    ///   默认 200 字符足以查看步骤目标和摘要，又不至于让日志被超长工具结果淹没。
    pub fn dump_history(&self, max_content_len: usize) {
        use tracing::info;

        info!(
            "========== msg_ctx 消息历史 dump 开始 (共 {} 条) ==========",
            self.messages.len()
        );

        for (i, msg) in self.messages.iter().enumerate() {
            let total_chars = msg.content.chars().count();
            let preview = if total_chars > max_content_len {
                let head: String = msg.content.chars().take(max_content_len).collect();
                format!("{}... [已截断，共 {} 字符]", head, total_chars)
            } else {
                msg.content.clone()
            };
            info!("[{:03}] [{:?}] {}", i, msg.role, preview);
        }

        info!("========== msg_ctx 消息历史 dump 结束 ==========");
    }
}
