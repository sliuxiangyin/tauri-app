//! 消息服务模块
//!
//! 提供轻量级的消息会话管理，直接写入数据库。
//!
//! ## 核心组件
//!
//! - [`MessagesSession`]: 会话管理器（保存 message_id 等状态，直接写 DB）
//! - [`MessageStatus`]: 消息状态枚举
//! - [`BlockAccumulator`]: 工具调用参数累加器
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! // 创建会话
//! let session = MessagesSession::new(
//!     account_id, chat_type, session_id, db_conn
//! ).await?;
//!
//! // 处理 LLM 流
//! while let Some(event) = stream.next().await {
//!     match event {
//!         LlmStreamEvent::TextDelta { text } => {
//!             session.add_text_block(&text).await;
//!         }
//!         LlmStreamEvent::Done => {
//!             session.complete(MessageStatus::Completed).await;
//!         }
//!         _ => {}
//!     }
//! }
//! ```

pub mod messages_event;
pub mod messages_session;

// Re-export commonly used types
pub use messages_event::{BlockAccumulator, MessageStatus};
pub use messages_session::MessagesSession;
