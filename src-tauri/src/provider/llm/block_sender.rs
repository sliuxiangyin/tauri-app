//! Block 发送器
//!
//! 封装 BlockStart 事件的发送逻辑：
//! - 自动递增 order_num
//! - 检测 block 类型切换（避免重复发送）
//! - 提供简单的 send(block_type) 接口

use crate::provider::llm::llm_event::{LlmStreamEvent, LlmStreamSender};

/// Block 发送器
///
/// 封装 BlockStart 事件的发送逻辑：
/// - 自动递增 order_num
/// - 提供简单的 send(block_type) 接口
/// - 支持状态追踪（检测 block 类型切换）
pub struct BlockSender {
    /// 当前 block 类型
    current_block_type: Option<String>,
    /// 下一 block 序号
    next_order_num: i32,
    /// 事件发送通道
    sender: Option<LlmStreamSender>,
}

impl BlockSender {
    /// 创建新的 BlockSender
    pub fn new(sender: Option<LlmStreamSender>) -> Self {
        Self {
            current_block_type: None,
            next_order_num: 1,
            sender,
        }
    }

    /// 发送 BlockStart 事件
    ///
    /// 如果 block_type 发生变化，则发送 BlockStart 事件并更新 order_num
    /// 如果 block_type 相同，则不发送（避免重复）
    ///
    /// # 返回
    /// 返回新分配的 order_num，如果类型相同则返回 None
    pub fn send(&mut self, block_type: &str) -> Option<i32> {
        // 检测 block 类型是否切换
        if self.current_block_type.as_deref() == Some(block_type) {
            // 类型相同，不发送
            return None;
        }

        let order_num = self.next_order_num;
        self.next_order_num += 1;
        self.current_block_type = Some(block_type.to_string());

        // 发送事件
        if let Some(ref s) = self.sender {
            let _ = s.send(LlmStreamEvent::BlockStart {
                block_type: block_type.to_string(),
                order_num,
            });
        }

        Some(order_num)
    }

    /// 获取当前 block 类型
    pub fn current_type(&self) -> Option<&str> {
        self.current_block_type.as_deref()
    }

    /// 获取下一 order_num（不递增）
    pub fn peek_order_num(&self) -> i32 {
        self.next_order_num
    }
}