//! 提示词模块
//!
//! 包含各类 LLM 提示词模板及相关工具函数

pub mod intent_prompt;

#[allow(unused_imports)]
pub use intent_prompt::{
    build_intent_user_message, build_tools_description, extract_user_request,
    intent_system_prompt, parse_intent_response,
};
