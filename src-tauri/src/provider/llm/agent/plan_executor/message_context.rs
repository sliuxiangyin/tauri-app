//! 探索性步骤消息上下文管理
//!
//! 统一管理 Agent 循环中的 LLM 对话消息列表，
//! 封装消息的构建与追加操作。

use crate::provider::llm::types::{ChatMessage, Role};

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
    pub fn new(push_step: &str) -> Self {
        let messages = vec![ChatMessage::new(Role::System, push_step)];

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
    pub fn push_tool_result(&mut self, name: &str, result: &str) {
        self.messages.push(ChatMessage::new(
            Role::Tool,
            &format!("工具 {} 执行结果: {}", name, result),
        ));
    }

    /// 添加目标判断结果到 LLM 消息历史
    pub fn push_goal_check(&mut self, achieved: bool, reason: &str) {
        self.messages.push(ChatMessage::new(
            Role::Assistant,
            &format!(
                "判断: {}\n原因: {}",
                if achieved { "YES" } else { "NO" },
                reason
            ),
        ));
    }
}
