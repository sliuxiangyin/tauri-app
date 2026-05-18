//! 聊天服务相关类型定义

use crate::provider::llm::types::ChatMessage;

/// 聊天上下文参数
/// 用于统一 client 和 wechat 渠道的 LLM 调用入口
#[derive(Debug, Clone)]
pub struct ChatContext {
    pub account_id: String,
    /// "client" 或 "wechat"
    pub chat_type: String,
    pub session_id: String,
    /// 当前请求的消息列表（包含 user 消息）
    pub messages: Vec<ChatMessage>,
}
