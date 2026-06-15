//! Agent 循环执行器
//!
//! 封装 LLM 循环调用逻辑：
//! - 调用 LLM stream_chat()
//! - 解析 ToolCall 事件
//! - 执行工具
//! - 注入 ToolResult 到消息历史
//! - 循环直到结束条件

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::provider::llm::agent::config::AgentConfig;
use crate::provider::llm::agent::event::{AgentResultSummary, AgentStreamEvent, StopReason};
use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
use crate::provider::llm::providers::provider_trait::{LlmProvider, LlmStream};
use crate::provider::llm::types::{ChatMessage, ChatRequest, FunctionCall, Role, ToolCallItem};

/// Agent 事件回调类型
///
/// 用于实时推送 AgentStreamEvent 给调用方。
pub type AgentEventCallback = Arc<dyn Fn(AgentStreamEvent) + Send + Sync>;

/// Agent 循环执行器
///
/// 使用示例（流式模式）：
/// ```ignore
/// let callback = |event| { /* 实时发送给前端 */ };
/// let runner = AgentRunner::new(provider, executor, config, messages)
///     .with_event_callback(callback);
/// 
/// let result = runner.run_streaming(request).await;
/// ```
#[allow(dead_code)]
pub struct AgentRunner<'a> {
    /// LLM Provider（实际使用 Arc<dyn LlmProvider>）
    provider: Arc<dyn LlmProvider + 'a>,
    /// 工具执行器
    tool_executor: Arc<dyn ToolExecutor>,
    /// Agent 配置
    config: AgentConfig,
    /// 消息历史（累积）
    messages: Vec<ChatMessage>,
    /// 当前步数
    step_count: AtomicU32,
    /// 中止标志（外部可设置）
    abort_flag: Arc<AtomicBool>,
    /// 开始时间（用于计算总超时）
    start_time: Instant,
    /// 连续空响应计数
    empty_response_count: AtomicU32,
    /// 连续错误计数
    error_count: AtomicU32,
    /// 总工具调用数
    tool_call_total: AtomicU32,
    /// 成功工具调用数
    tool_call_success: AtomicU32,
    /// 失败工具调用数
    tool_call_failed: AtomicU32,
    /// 事件回调（实时推送流式事件）
    event_callback: Option<AgentEventCallback>,
    /// LLM 文本累积（用于检测空响应）
    llm_text_buffer: String,
}

impl<'a> AgentRunner<'a> {
    /// 创建 AgentRunner
    pub fn new(
        provider: Arc<dyn LlmProvider + 'a>,
        tool_executor: Arc<dyn ToolExecutor>,
        config: AgentConfig,
        initial_messages: Vec<ChatMessage>,
    ) -> Self {
        Self {
            provider,
            tool_executor,
            config,
            messages: initial_messages,
            step_count: AtomicU32::new(0),
            abort_flag: Arc::new(AtomicBool::new(false)),
            start_time: Instant::now(),
            empty_response_count: AtomicU32::new(0),
            error_count: AtomicU32::new(0),
            tool_call_total: AtomicU32::new(0),
            tool_call_success: AtomicU32::new(0),
            tool_call_failed: AtomicU32::new(0),
            event_callback: None,
            llm_text_buffer: String::new(),
        }
    }

    /// 设置事件回调（用于实时推送流式事件）
    pub fn with_event_callback(mut self, callback: AgentEventCallback) -> Self {
        self.event_callback = Some(callback);
        self
    }

    /// 使用 Arc<AtomicBool> 作为中止标志
    pub fn with_abort_flag(mut self, abort_flag: Arc<AtomicBool>) -> Self {
        self.abort_flag = abort_flag;
        self
    }
    pub fn abort_flag(&self) -> Arc<AtomicBool> {
        self.abort_flag.clone()
    }

    /// 发送事件到回调
    fn emit(&self, event: AgentStreamEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }

    /// 流式执行 Agent 循环（实时推送事件）
    ///
    /// 与 `run()` 的区别：
    /// - 实时推送 LLM TextDelta 事件
    /// - 实时推送工具执行开始/完成事件
    /// - 每步完成后推送 StepComplete 事件
    pub async fn run_streaming(
        mut self,
        mut req: ChatRequest,
    ) -> Result<(Vec<ChatMessage>, AgentResultSummary), AgentError> {
        // 发送 Agent 开始事件
        info!(
            "AgentRunner streaming started: max_steps={}, timeout_total={:?}",
            self.config.max_steps,
            self.config.timeout_total
        );

        self.emit(AgentStreamEvent::AgentStart { step: 0 });

        // 更新初始消息
        req.messages = self.messages.clone();

        // 循环执行
        loop {
            // 1. 检查终止条件
            if let Some(stop_reason) = self.should_stop() {
                info!("Agent stopped: {:?}", stop_reason);
                self.emit(AgentStreamEvent::AgentComplete {
                    total_steps: self.step_count.load(Ordering::SeqCst),
                    stop_reason,
                    final_content: self.build_final_content(),
                });
                return Ok((self.messages.clone(), self.build_summary(stop_reason)));
            }

            // 2. 检查总超时
            if self.start_time.elapsed() > self.config.timeout_total {
                warn!("Agent reached total timeout");
                self.emit(AgentStreamEvent::AgentComplete {
                    total_steps: self.step_count.load(Ordering::SeqCst),
                    stop_reason: StopReason::TimeoutReached,
                    final_content: self.build_final_content(),
                });
                return Ok((self.messages.clone(), self.build_summary(StopReason::TimeoutReached)));
            }

            // 3. 增加步数
            let current_step = self.step_count.fetch_add(1, Ordering::SeqCst) + 1;
            debug!("Agent step {} starting", current_step);
            self.emit(AgentStreamEvent::StepStart { step: current_step });

            // 4. 清空文本缓冲
            self.llm_text_buffer.clear();

            // 5. 构建请求（更新消息）
            req.messages = self.messages.clone();

            // 6. 调用 LLM
            let stream_result = self.provider.stream_chat(req.clone(), self.abort_flag.clone()).await;

            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    error!("LLM stream error: {}", e);
                    self.error_count.fetch_add(1, Ordering::SeqCst);
                    self.emit(AgentStreamEvent::Llm(LlmStreamEvent::Error {
                        code: "llm_error".to_string(),
                        message: e.to_string(),
                    }));
                    return Err(AgentError::LlmError(e));
                }
            };

            // 7. 处理流式响应（实时转发 + 收集工具调用）
            let tool_calls = match self.process_stream_live(stream).await {
                Ok(tc) => tc,
                Err(e) => {
                    error!("Stream processing error: {}", e);
                    self.error_count.fetch_add(1, Ordering::SeqCst);
                    return Err(e);
                }
            };

            // 8. 检查空响应
            if self.llm_text_buffer.is_empty() && tool_calls.is_empty() {
                self.empty_response_count.fetch_add(1, Ordering::SeqCst);
                debug!("Step {}: empty response", current_step);
            } else {
                self.empty_response_count.store(0, Ordering::SeqCst);
            }

            // 9. 执行工具调用
            let had_tool_call = !tool_calls.is_empty();
            let tool_call_count = tool_calls.len() as u32;

            // 创建 assistant 消息（同时包含文本和工具调用）
            if !self.llm_text_buffer.is_empty() || !tool_calls.is_empty() {
                // 将 FunctionCall 转换为 ToolCallItem
                let tool_call_items: Vec<ToolCallItem> = tool_calls.iter().map(|tc| {
                    ToolCallItem {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: serde_json::to_string(&tc.arguments).unwrap_or_else(|_| "{}".to_string()),
                    }
                }).collect();

                let assistant_msg = ChatMessage {
                    role: Role::Assistant,
                    content: self.llm_text_buffer.clone(),
                    tool_call_id: None,
                    name: None,
                    tool_calls: if tool_call_items.is_empty() { None } else { Some(tool_call_items) },
                };
                self.messages.push(assistant_msg);
                debug!("Step {}: added assistant message (text={} chars, tools={})", 
                    current_step, self.llm_text_buffer.len(), tool_calls.len());
            }

            for tc in &tool_calls {
                debug!("Executing tool call: {} (id={})", tc.name, tc.id);
                
                // 发送工具开始事件
                self.emit(AgentStreamEvent::ToolStart {
                    call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: Some(serde_json::to_string(&tc.arguments).unwrap_or_default()),
                });

                let start_time = Instant::now();
                let result = self.execute_tool_internal(tc.clone()).await;

                // 记录成功/失败
                match &result {
                    Ok(_) => {
                        self.tool_call_success.fetch_add(1, Ordering::SeqCst);
                        self.emit(AgentStreamEvent::ToolComplete {
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            duration_ms: start_time.elapsed().as_millis() as u64,
                            success: true,
                        });
                    }
                    Err(e) => {
                        self.tool_call_failed.fetch_add(1, Ordering::SeqCst);
                        self.emit(AgentStreamEvent::ToolError {
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            error: e.to_string(),
                        });
                    }
                }

                // 10. 注入工具结果到消息历史
                let content = match result {
                    Ok(v) => serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => format!("Error: {}", e),
                };
                let msg = ChatMessage::tool_result(&tc.id, &tc.name, content);
                self.messages.push(msg);
            }

            // 11. 记录工具调用统计
            self.tool_call_total.fetch_add(tool_call_count, Ordering::SeqCst);

            // 12. 发送步骤完成事件
            self.emit(AgentStreamEvent::StepComplete {
                step: current_step,
                had_tool_call,
                tool_call_count,
            });

            // 13. 检查是否需要继续循环
            if !had_tool_call && self.llm_text_buffer.is_empty() {
                debug!("Step {} completed without tool call, checking for completion", current_step);
            }

            if current_step >= self.config.max_steps {
                debug!("Max steps reached after step {}", current_step);
            }
        }
    }

    /// 构建最终内容摘要
    fn build_final_content(&self) -> Option<String> {
        self.messages.last().map(|m| {
            if m.content.len() > 500 {
                format!("{}... (truncated)", &m.content[..500])
            } else {
                m.content.clone()
            }
        })
    }

    /// 处理流式响应（实时转发 + 收集工具调用）
    async fn process_stream_live(
        &mut self,
        stream: LlmStream,
    ) -> Result<Vec<FunctionCall>, AgentError> {
        let mut tool_calls: Vec<FunctionCall> = Vec::new();
        let mut current_call: Option<PendingToolCall> = None;

        futures_util::pin_mut!(stream);

        while let Some(item) = stream.next().await {
            match item {
                Ok(event) => {
                    match event {
                        LlmStreamEvent::TextDelta { text } => {
                            self.llm_text_buffer.push_str(&text);
                        }
                        LlmStreamEvent::ToolCallStart { index, id, name } => {
                            // 如果有待处理的工具调用，先保存
                            if let Some(tc) = current_call.take() {
                                if let Some(call) = tc.finalize() {
                                    tool_calls.push(call);
                                }
                            }
                            // 开始新的工具调用
                            current_call = Some(PendingToolCall {
                                index,
                                id,
                                name,
                                arguments: String::new(),
                            });
                        }
                        LlmStreamEvent::ToolCallDelta { index: _, arguments } => {
                            if let Some(ref mut tc) = current_call {
                                tc.arguments.push_str(&arguments);
                            }
                        }
                        LlmStreamEvent::ToolCallDone { index: _, arguments } => {
                            // 参数合并到 current_call
                            if let Some(ref mut tc) = current_call {
                                // 尝试解析已累积的参数
                                tc.arguments.push_str(&arguments.to_string());
                            }
                        }
                        LlmStreamEvent::ToolResult { .. } => {
                            // Agent 循环中不应收到 ToolResult（这是我们注入的）
                        }
                        LlmStreamEvent::Done => {
                            // 流结束
                            break;
                        }
                        LlmStreamEvent::Error { code, message } => {
                            warn!("LLM stream error: {} - {}", code, message);
                        }
                        // 其他事件（ReasoningDelta, Reference, Audio 等）
                        // 不影响工具调用收集
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("Stream item error: {}", e);
                    return Err(AgentError::StreamError(e));
                }
            }
        }

        // 处理最后一个工具调用
        if let Some(tc) = current_call.take() {
            if let Some(call) = tc.finalize() {
                tool_calls.push(call);
            }
        }

        Ok(tool_calls)
    }

    /// 内部执行工具
    async fn execute_tool_internal(&self, call: FunctionCall) -> Result<Value, ToolExecError> {
        self.tool_executor.execute_tool(call).await
    }

    /// 检查是否应该停止循环
    fn should_stop(&self) -> Option<StopReason> {
        // 用户中止
        if self.abort_flag.load(Ordering::SeqCst) {
            return Some(StopReason::UserAbort);
        }

        // 最大步数
        if self.step_count.load(Ordering::SeqCst) >= self.config.max_steps {
            return Some(StopReason::MaxStepsReached);
        }

        // 空响应阈值
        if self.empty_response_count.load(Ordering::SeqCst) >= self.config.empty_response_threshold {
            return Some(StopReason::EmptyResponseThreshold);
        }

        // 错误阈值
        if self.error_count.load(Ordering::SeqCst) >= self.config.error_threshold {
            return Some(StopReason::ErrorThreshold);
        }

        None
    }

    /// 记录空响应
    pub fn record_empty_response(&self) {
        self.empty_response_count.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录错误
    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录成功
    pub fn record_success(&self) {
        self.empty_response_count.store(0, Ordering::SeqCst);
        self.error_count.store(0, Ordering::SeqCst);
    }

    /// 记录工具执行成功
    pub fn record_tool_success(&self) {
        self.tool_call_success.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录工具执行失败
    pub fn record_tool_failure(&self) {
        self.tool_call_failed.fetch_add(1, Ordering::SeqCst);
    }

    /// 构建结果摘要
    fn build_summary(&self, stop_reason: StopReason) -> AgentResultSummary {
        let total_steps = self.step_count.load(Ordering::SeqCst);
        let final_content = self.messages.last().map(|m| {
            if m.content.len() > 500 {
                format!("{}... (truncated)", &m.content[..500])
            } else {
                m.content.clone()
            }
        });

        AgentResultSummary {
            total_steps,
            stop_reason,
            total_tool_calls: self.tool_call_total.load(Ordering::SeqCst),
            successful_tool_calls: self.tool_call_success.load(Ordering::SeqCst),
            failed_tool_calls: self.tool_call_failed.load(Ordering::SeqCst),
            total_duration_ms: self.start_time.elapsed().as_millis() as u64,
            final_content,
            error: None,
        }
    }
}

/// 待处理的工具调用（用于累积参数）
#[allow(dead_code)]
struct PendingToolCall {
    index: u32,
    id: String,
    name: String,
    arguments: String,
}

impl PendingToolCall {
    /// 完成工具调用，解析参数
    fn finalize(self) -> Option<FunctionCall> {
        let arguments = if self.arguments.is_empty() {
            Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&self.arguments).unwrap_or_else(|_| {
                // 尝试修复 JSON（有些 LLM 返回的 JSON 可能有问题）
                let cleaned = self.arguments.trim();
                serde_json::from_str(cleaned).unwrap_or_else(|_| Value::Object(serde_json::Map::new()))
            })
        };

        Some(FunctionCall {
            id: self.id,
            name: self.name,
            arguments,
        })
    }
}

/// Agent 执行错误
#[derive(Debug)]
#[allow(dead_code)]
pub enum AgentError {
    /// LLM 错误
    LlmError(LlmError),
    /// 流处理错误
    StreamError(LlmError),
    /// 工具执行错误
    ToolError(ToolExecError),
    /// 配置错误
    Config(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::LlmError(e) => write!(f, "LLM error: {}", e),
            AgentError::StreamError(e) => write!(f, "Stream error: {}", e),
            AgentError::ToolError(e) => write!(f, "Tool error: {}", e),
            AgentError::Config(e) => write!(f, "Config error: {}", e),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<LlmError> for AgentError {
    fn from(e: LlmError) -> Self {
        AgentError::LlmError(e)
    }
}

impl From<ToolExecError> for AgentError {
    fn from(e: ToolExecError) -> Self {
        AgentError::ToolError(e)
    }
}

/// Agent 事件发送器
///
/// 用于将 AgentStreamEvent 发送到 channel，供外部消费。
pub struct AgentEventSender {
    sender: mpsc::UnboundedSender<AgentStreamEvent>,
}

impl AgentEventSender {
    /// 创建新的事件发送器
    pub fn new() -> (Self, mpsc::UnboundedReceiver<AgentStreamEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { sender: tx }, rx)
    }

    /// 发送事件
    pub fn send(&self, event: AgentStreamEvent) {
        let _ = self.sender.send(event);
    }

    /// 检查是否已关闭
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

impl Default for AgentEventSender {
    fn default() -> Self {
        let (tx, _rx) = mpsc::unbounded_channel::<AgentStreamEvent>();
        Self { sender: tx }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        use std::time::Duration;
        let config = AgentConfig::default();
        assert_eq!(config.max_steps, 10);
        assert_eq!(config.timeout_per_step, Duration::from_secs(60));
        assert_eq!(config.timeout_total, Duration::from_secs(300));
    }

    #[test]
    fn test_agent_config_builder() {
        use std::time::Duration;
        let config = AgentConfig::new()
            .with_max_steps(20)
            .with_timeout_total(Duration::from_secs(600));

        assert_eq!(config.max_steps, 20);
        assert_eq!(config.timeout_total, Duration::from_secs(600));
    }
}