//! LLM Agent 基础设施
//!
//! 提供所有 LLM Agent 共用的基类和 Trait：
//! - [`LlmAgentBase`]：持有 provider 和通用参数（model / temperature / max_tokens）
//! - [`LlmAgent`]：Trait，约束 build_messages() / parse_response()
//! - [`run_llm`] / [`run_streaming_llm`]：共享的非流式/流式执行逻辑
//!
//! ## 架构
//!
//! 所有 Agent 都嵌入 [`LlmAgentBase`]，并实现 [`LlmAgent`] Trait。
//! `run()` / `run_streaming()` 由各 Agent 自行实现，内部调用共享辅助函数：
//!
//! ```ignore
//! pub struct TaskPlannerAgent {
//!     base: LlmAgentBase,
//!     user_request: String,
//! }
//!
//! impl LlmAgent for TaskPlannerAgent {
//!     fn build_messages(&self) -> Vec<ChatMessage> { ... }
//!     fn parse_response(&self, response: &str) -> Result<TaskPlan, LlmError> { ... }
//! }
//!
//! impl TaskPlannerAgent {
//!     pub async fn run(&self) -> Result<TaskPlan, LlmError> {
//!         let messages = self.build_messages();
//!         run_llm(self.base(), messages, |r| self.parse_response(r)).await
//!     }
//! }
//! ```

use std::sync::Arc;

use async_stream::stream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::providers::provider_trait::{LlmProvider, LlmStream};
use crate::provider::llm::types::ChatMessage;

/// LLM 流式响应
///
/// 包含两路输出：
/// - `text_stream`：实时 `TextDelta` 事件，前端逐字展示 LLM 输出
/// - `parse_future`：后台线程累积完整文本并解析为结构化结果，调用方 await 获得
pub struct StreamingResponse<T> {
    /// LLM 输出流（`TextDelta` 事件），前端用于实时渲染
    pub text_stream: LlmStream,
    /// 后台解析 Future，等待完成后返回结构化结果
    pub parse_future: JoinHandle<Result<T, LlmError>>,
}

// ──────────────────────────────────────────────────────────────
// LlmAgentBase：共享参数持有者
// ──────────────────────────────────────────────────────────────

/// LLM Agent 基类
///
/// 持有所有 Agent 共用的参数：
/// - `provider`：LLM 调用后端
/// - `model`：模型名称（空字符串时 fallback 到 provider 默认模型）
/// - `temperature`：采样温度
/// - `max_tokens`：最大输出 token
///
/// 具体 Agent 通过嵌入此结构体并实现 [`LlmAgent`] Trait 来获得统一行为。
///
/// # 示例
///
/// ```ignore
/// pub struct TaskPlannerAgent {
///     base: LlmAgentBase,
///     // ... 其他字段
/// }
///
/// impl LlmAgent for TaskPlannerAgent {
///     fn build_messages(&self) -> Vec<ChatMessage> { ... }
///     fn parse_response(&self, response: &str) -> Result<TaskPlan, LlmError> { ... }
/// }
/// ```
#[derive(Clone)]
pub struct LlmAgentBase {
    /// LLM Provider
    provider: Arc<dyn LlmProvider>,
    /// 模型名称（空字符串时 fallback 到 provider 默认模型）
    model: String,
    /// 采样温度
    temperature: f32,
    /// 最大输出 token
    max_tokens: Option<u32>,
}

impl LlmAgentBase {
    /// 创建 LlmAgentBase（使用 provider 默认模型）
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            model: String::new(),
            temperature: 0.1,
            max_tokens: Some(8192),
        }
    }

    /// 设置模型
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// 设置温度
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// 设置最大 tokens
    pub fn with_max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// 获取模型（外部传入优先，fallback 到 provider 默认模型）
    pub fn resolve_model(&self) -> Result<String, LlmError> {
        if self.model.is_empty() {
            self.provider
                .default_model()
                .map(String::from)
                .ok_or_else(|| LlmError::Config("Model not set for LLM agent".into()))
        } else {
            Ok(self.model.clone())
        }
    }

    /// 构建 ChatRequest
    pub(crate) fn build_request(&self, messages: Vec<ChatMessage>, model: String) -> crate::provider::llm::types::ChatRequest {
        crate::provider::llm::types::ChatRequest {
            messages,
            model,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            tools: None,
        }
    }

    /// 获取 provider 引用
    pub fn provider(&self) -> &Arc<dyn LlmProvider> {
        &self.provider
    }
}

// ──────────────────────────────────────────────────────────────
// LlmAgent Trait：所有 LLM Agent 必须实现
// ──────────────────────────────────────────────────────────────

/// LLM Agent Trait
///
/// 所有 Agent 必须实现此 Trait，提供消息构建和响应解析能力。
/// `run()` / `run_streaming()` 由各 Agent 自行实现，内部可调用 [`run_llm`] / [`run_streaming_llm`]。
pub trait LlmAgent: Send + Sync {
    /// Agent 输出的结构化类型
    type Output: Send + 'static;

    /// 构建消息列表
    fn build_messages(&self) -> Vec<ChatMessage>;

    /// 解析 LLM 返回的文本为结构化结果
    fn parse_response(&self, response: &str) -> Result<Self::Output, LlmError>;

    /// 获取基类引用
    fn base(&self) -> &LlmAgentBase;
}

// ──────────────────────────────────────────────────────────────
// 共享辅助函数：run_llm / run_streaming_llm
// ──────────────────────────────────────────────────────────────

/// 非流式执行：调用 LLM 并解析响应
///
/// 各 Agent 的 `run()` 方法内部调用此函数。
pub async fn run_llm<T, F>(
    base: &LlmAgentBase,
    messages: Vec<ChatMessage>,
    parser: F,
) -> Result<T, LlmError>
where
    F: FnOnce(&str) -> Result<T, LlmError>,
{
    let model = base.resolve_model()?;
    let req = base.build_request(messages, model);
    let response = base.provider().send_message(req).await?;
    parser(&response)
}

/// 流式执行：调用 LLM 流式接口，返回 text_stream + parse_future
///
/// 各 Agent 的 `run_streaming()` 方法内部调用此函数。
pub async fn run_streaming_llm<T, F>(
    base: &LlmAgentBase,
    messages: Vec<ChatMessage>,
    parser: F,
) -> Result<StreamingResponse<T>, LlmError>
where
    T: Send + 'static,
    F: FnOnce(&str) -> Result<T, LlmError> + Send + 'static,
{
    let model = base.resolve_model()?;
    let stream_req = base.build_request(messages, model);

    // 通道1：text_tx -> text_rx（转发所有事件给 text_stream）
    let (text_tx, text_rx) =
        mpsc::unbounded_channel::<Result<LlmStreamEvent, LlmError>>();
    // 通道2：full_tx -> full_rx（累积完整文本给 parse_future）
    let (full_tx, full_rx) = mpsc::unbounded_channel::<String>();

    // text_stream：把 text_rx 包装为 LlmStream
    let text_stream: LlmStream = Box::pin(stream! {
        let mut rx = text_rx;
        while let Some(event) = rx.recv().await {
            yield event;
        }
    });

    // parse_future：累积完整文本并解析
    let parse_future = tokio::spawn(async move {
        let mut full_rx = full_rx;
        let mut full_text = String::new();
        while let Some(delta) = full_rx.recv().await {
            full_text.push_str(&delta);
        }
        if full_text.is_empty() {
            return Err(LlmError::EmptyResponse);
        }
        parser(&full_text)
    });

    // 流式消费任务：把 provider 流式事件分发到 text_tx 和 full_tx
    let provider = base.provider().clone();
    let abort_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    tokio::spawn(async move {
        let stream = provider.stream_chat(stream_req, abort_flag).await;
        match stream {
            Ok(mut s) => {
                while let Some(event) = futures_util::StreamExt::next(&mut s).await {
                    match event {
                        Ok(evt) => {
                            if let LlmStreamEvent::TextDelta { text } = &evt {
                                let _ = full_tx.send(text.clone());
                            }
                            let _ = text_tx.send(Ok(evt.clone()));
                            if matches!(evt, LlmStreamEvent::Done) {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = text_tx.send(Err(e));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                let _ = text_tx.send(Err(e));
            }
        }
        // drop full_tx and text_tx automatically when scope ends
    });

    Ok(StreamingResponse {
        text_stream,
        parse_future,
    })
}
