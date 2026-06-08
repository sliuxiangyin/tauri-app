//! 消息会话辅助类型
//!
//! 提供 MessageStatus、BlockInfo、ToolCallRecord 和 BlockAccumulator 供 MessagesSession 使用

use serde::{Deserialize, Serialize};

// 重新导出 ToolCallRecord（定义在 provider::llm::types）
pub use crate::provider::llm::types::ToolCallRecord;

/// 消息状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    Pending,
    Completed,
    Failed,
}

impl MessageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageStatus::Pending => "pending",
            MessageStatus::Completed => "completed",
            MessageStatus::Failed => "failed",
        }
    }
}

/// Block 信息（用于前端渲染 block 边界）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    /// Block 类型：text, thinking, tool_call, tool_result
    pub block_type: String,
    /// 块序号（自动递增）
    pub order_num: i32,
}

impl BlockInfo {
    pub fn new(block_type: &str, order_num: i32) -> Self {
        Self {
            block_type: block_type.to_string(),
            order_num,
        }
    }
}

/// 块累加器（用于聚合工具调用增量参数）
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct BlockAccumulator {
    /// 文本内容累积
    pub text: String,
    /// 思考过程累积
    pub thinking: String,
    /// 工具名称
    pub tool_name: Option<String>,
    /// 工具参数累积
    pub tool_arguments: String,
}

#[allow(dead_code)]
impl BlockAccumulator {
    /// 添加文本
    pub fn add_text(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// 添加思考内容
    pub fn add_thinking(&mut self, text: &str) {
        self.thinking.push_str(text);
    }

    /// 添加工具参数
    pub fn add_arguments(&mut self, args: &str) {
        self.tool_arguments.push_str(args);
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.thinking.is_empty() && self.tool_arguments.is_empty()
    }

    /// 重置
    pub fn clear(&mut self) {
        self.text.clear();
        self.thinking.clear();
        self.tool_arguments.clear();
    }
}
