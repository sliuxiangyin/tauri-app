use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use crate::provider::llm::block_sender::BlockSender;
use crate::provider::llm::error::LlmError;
use crate::provider::llm::types::FunctionCall;

/// LLM 流式事件类型，定义了从 LLM 流式响应中解析出的所有可能事件。
/// 由命令层包成 [`LlmChunkEnvelope`] 后通过 Tauri emit 推送到前端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LlmStreamEvent {
    // ========== 文本内容 ==========
    /// 普通文本增量，模型输出的文本片段
    TextDelta { text: String },

    // ========== 思考链（Reasoning） ==========
    /// 思考链增量，如 OpenAI o1/o3、Anthropic Claude 的 thinking 过程
    ReasoningDelta { text: String },

    // ========== 工具调用（Tool Calls） ==========
    /// 工具调用开始，包含工具索引、调用 ID 和工具名称
    ToolCallStart {
        /// 工具调用索引，支持并行调用
        index: u32,
        /// 工具调用唯一 ID，用于标识此次调用
        id: String,
        /// 工具名称
        name: String,
    },
    /// 工具调用参数增量，streaming 模式下参数分片传输，需前端拼接
    ToolCallDelta {
        /// 工具调用索引
        index: u32,
        /// 参数增量片段（需累积拼接为完整 JSON 字符串）
        arguments: String,
    },
    /// 工具调用完成，包含完整的函数参数
    ToolCallDone {
        /// 工具调用索引
        index: u32,
        /// 完整的函数参数（JSON 对象）
        arguments: Value,
    },
    /// 工具执行结果（由调用方执行工具后产生，非 LLM 直接输出）
    /// 通常在 Agent 循环中由外部注入，用于告知 LLM 工具执行结果
    ToolResult {
        /// 工具调用 ID，与 ToolCallStart 中的 id 对应
        call_id: String,
        /// 工具名称
        name: String,
        /// 执行结果内容（成功时为结果，失败时为错误信息）
        result: Value,
        /// 是否执行成功
        success: bool,
    },

    // ========== 引用（References） ==========
    /// 引用文档/来源，如 Deep Research 场景中的参考资料
    Reference {
        /// 引用类型：如 "url", "file", "document"
        source_type: String,
        /// 引用标题
        title: String,
        /// 引用链接或路径
        url: String,
        /// 引用片段摘要
        snippet: Option<String>,
    },

    // ========== 音频（Audio） ==========
    /// 音频增量，如 gpt-4o-audio-preview 模型的语音输出
    AudioDelta {
        /// 音频数据（Base64 编码或二进制流）
        data: String,
        /// 音频格式：如 "mp3", "wav", "pcm"
        format: String,
    },

    // ========== 错误与警告 ==========
    /// 流式处理中的错误事件（如连接断开、解析失败等）
    Error {
        /// 错误代码
        code: String,
        /// 错误消息
        message: String,
    },
    /// 警告信息（如速率限制、性能建议等）
    Warning {
        /// 警告代码
        code: String,
        /// 警告消息
        message: String,
    },

    // ========== 元数据与统计 ==========
    /// Token 使用量统计，通常在流结束时或分块累计时发送
    Usage {
        /// 输入 token 数量
        input_tokens: u32,
        /// 输出 token 数量
        output_tokens: u32,
        /// 思考 token 数量（如适用）
        reasoning_tokens: Option<u32>,
    },
    /// 流式响应元数据
    Metadata {
        /// 模型 ID
        model: String,
        /// 完成原因：如 "stop", "length", "content_filter"
        finish_reason: Option<String>,
        /// 请求 ID（用于追踪）
        request_id: Option<String>,
    },

    // ========== 结束标记 ==========
    /// 流式响应完成标记，表示 LLM 已完成所有输出
    Done,

    // ========== Block 边界标记 ==========
    /// Block 开始标记，标识一个新的内容块
    ///
    /// 用于前端区分不同 block 的边界，便于渲染和交互。
    /// 当 LLM 开始输出新的内容块（文本/思考/工具调用）时发送。
    BlockStart {
        /// Block 类型：text, thinking, tool_call, tool_result
        block_type: String,
        /// 块序号（自动递增）
        order_num: i32,
    },

    // ========== Agent Plan 相关 ==========
    /// Plan 步骤列表（Agent 模式进入时推送，供前端渲染步骤卡片）
    PlanSteps {
        /// LLM 判断理由
        reasoning: String,
        /// 执行步骤列表
        steps: Vec<crate::provider::llm::types::PlanStep>,
    },
}

/// 流式事件发送端：由调用方创建并传入，供 `stream_chat` 内部转发流式片段。
pub type LlmStreamSender = tokio::sync::mpsc::UnboundedSender<LlmStreamEvent>;

/// 推送到前端的 `llm:chunk` 载荷：固定带 `account_id`，便于 `listen` 里按账号过滤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmChunkEnvelope {
    pub account_id: String,
    #[serde(flatten)]
    pub event: LlmStreamEvent,
}

impl LlmChunkEnvelope {
    pub fn new(account_id: impl Into<String>, event: LlmStreamEvent) -> Self {
        Self {
            account_id: account_id.into(),
            event,
        }
    }
}

// =============================================================================
// 工具调用记录和返回结果
// =============================================================================

pub use crate::provider::llm::types::ToolCallRecord;

/// 流式处理最终结果
///
/// 包含 LLM 回复文本和完整的工具调用记录（含执行结果）。
/// 供调用方统一入库使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    /// 最终文本回复
    pub text: String,
    /// 工具调用记录列表（包含调用信息 + 执行结果）
    pub tool_calls: Vec<ToolCallRecord>,
}

// =============================================================================
// ToolExecutor trait - 统一工具执行接口
// =============================================================================

/// 工具执行错误
#[derive(Debug, Clone)]
pub struct ToolExecError {
    pub name: String,
    pub message: String,
}

impl std::fmt::Display for ToolExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tool '{}' execution failed: {}", self.name, self.message)
    }
}

impl std::error::Error for ToolExecError {}

impl From<ToolExecError> for String {
    fn from(e: ToolExecError) -> Self {
        e.to_string()
    }
}

/// 工具执行器 trait
///
/// 由外部（如 MCP Manager、Skills Manager）实现，负责执行具体的工具调用。
/// 支持多种工具源（MCP、Skills 等）。
///
/// # 使用方式
/// 1. 实现 trait（适合复杂逻辑或需要依赖注入）
/// 2. 传入 LLM Provider 或 Service 层
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 执行工具调用
    ///
    /// # 参数
    /// - `call`: 函数调用信息（包含 id、name、arguments）
    ///
    /// # 返回
    /// 执行结果（JSON Value）。
    async fn execute(&self, call: FunctionCall) -> Result<Value, ToolExecError>;
}

// =============================================================================
// 工具执行辅助函数（工具解析工具参数等，供 service 层使用）
// =============================================================================

/// 工具执行上下文
///
/// 用于在流式处理中追踪工具执行状态。
pub struct ToolExecContext {
    /// 当前批次是否有过工具调用
    pub had_tool_call: bool,
    /// 总调用数
    pub total_calls: u32,
    /// 成功调用数
    pub successful_calls: u32,
    /// 失败调用数
    pub failed_calls: u32,
    /// 开始时间
    start_time: Instant,
}

impl ToolExecContext {
    /// 创建新的执行上下文
    pub fn new() -> Self {
        Self {
            had_tool_call: false,
            total_calls: 0,
            successful_calls: 0,
            failed_calls: 0,
            start_time: Instant::now(),
        }
    }

    /// 记录一次工具调用
    pub fn record_call(&mut self, success: bool) {
        self.had_tool_call = true;
        self.total_calls += 1;
        if success {
            self.successful_calls += 1;
        } else {
            self.failed_calls += 1;
        }
    }

    /// 获取总执行耗时
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }
}

impl Default for ToolExecContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 流式处理工具调用批次
///
/// 在 LLM 流中累积工具调用，完成后执行并返回完整结果。
///
/// # 参数
/// - `stream`: LLM 流
/// - `executor`: 工具执行器（可选）
/// - `sender`: 流式事件发送通道（可选，用于发送给前端）
///
/// # 返回
/// 返回 `ProcessResult`，包含最终文本、工具调用记录（含执行结果）和执行统计。
pub async fn process_tool_batch(
    stream: LlmStream,
    executor: Option<Arc<dyn ToolExecutor>>,
    sender: Option<&tokio::sync::mpsc::UnboundedSender<LlmStreamEvent>>,
) -> Result<ProcessResult, LlmError> {
    let mut pending_calls: Vec<ToolCallRecord> = Vec::new();
    let mut current_call: Option<PendingToolCall> = None;
    let mut exec_ctx = ToolExecContext::new();
    let mut final_reply = String::new();

    futures_util::pin_mut!(stream);
    let mut block_sender = BlockSender::new(sender.cloned());
    block_sender.send("text");

    while let Some(item) = stream.next().await {
        match item {
            Ok(event) => {
                match event {
                    LlmStreamEvent::TextDelta { text } => {
                        final_reply.push_str(&text);
                        // 转发 TextDelta 给前端
                        if let Some(ref s) = sender {
                            let _ = s.send(LlmStreamEvent::TextDelta { text });
                        }
                    }
                    LlmStreamEvent::ToolCallStart { index, id, name } => {
                        block_sender.send("tool_call");
                        // 转发 ToolCallStart 给前端
                        if let Some(ref s) = sender {
                            let _ = s.send(LlmStreamEvent::ToolCallStart {
                                index,
                                id: id.clone(),
                                name: name.clone(),
                            });
                        }
                        // 先保存上一个工具调用
                        if let Some(tc) = current_call.take() {
                            if let Some(record) = tc.finalize() {
                                pending_calls.push(record);
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
                    LlmStreamEvent::ToolCallDelta { index, arguments } => {
                        if let Some(ref mut tc) = current_call {
                            tc.arguments.push_str(&arguments);
                        }
                        // 转发 ToolCallDelta 给前端
                        if let Some(ref s) = sender {
                            let _ = s.send(LlmStreamEvent::ToolCallDelta { index, arguments });
                        }
                    }
                    LlmStreamEvent::ToolCallDone { index, arguments } => {
                        if let Some(ref mut tc) = current_call {
                            tc.arguments.push_str(&arguments.to_string());
                        }
                        // 转发 ToolCallDone 给前端
                        if let Some(ref s) = sender {
                            let _ = s.send(LlmStreamEvent::ToolCallDone {
                                index,
                                arguments: arguments.clone(),
                            });
                        }
                    }
                    LlmStreamEvent::Done => break,
                    // 转发其他事件
                    _ => {
                        if let Some(ref s) = sender {
                            let _ = s.send(event);
                        }
                    }
                }
            }
            Err(e) => return Err(e),
        }
    }

    // 处理最后一个工具调用
    if let Some(tc) = current_call.take() {
        if let Some(record) = tc.finalize() {
            pending_calls.push(record);
        }
    }
    tracing::info!("[LLM] Tool pending_calls: {:?}", pending_calls);
    // 执行工具调用并填充结果
    let mut tool_calls: Vec<ToolCallRecord> = Vec::new();
    block_sender.send("tool_result");
    if let Some(ref exec) = executor {
        for mut call in pending_calls {
            // 执行工具
            match exec.execute(call.clone().into()).await {
                Ok(result) => {
                    exec_ctx.record_call(true);
                    call.success = true;
                    call.result = Some(result.clone());
                    if let Some(ref s) = sender {
                        let _ = s.send(LlmStreamEvent::ToolResult {
                            call_id: call.call_id.clone(),
                            name: call.name.clone(),
                            result,
                            success: true,
                        });
                    }
                }
                Err(e) => {
                    exec_ctx.record_call(false);
                    let error_result = Value::String(e.to_string());
                    call.success = false;
                    call.result = Some(error_result.clone());
                    if let Some(ref s) = sender {
                        let _ = s.send(LlmStreamEvent::ToolResult {
                            call_id: call.call_id.clone(),
                            name: call.name.clone(),
                            result: error_result,
                            success: false,
                        });
                    }
                }
            }
            tool_calls.push(call);
        }
    } else {
        // 没有 executor 时，为每个待执行的工具调用发送失败结果
        for call in pending_calls {
            let error_result = Value::String("No executor available".to_string());
            if let Some(ref s) = sender {
                let _ = s.send(LlmStreamEvent::ToolResult {
                    call_id: call.call_id.clone(),
                    name: call.name.clone(),
                    result: error_result,
                    success: false,
                });
            }
            tool_calls.push(call);
        }
    }
    tracing::info!("[LLM] Tool calls: {:?}", tool_calls);

    Ok(ProcessResult {
        text: final_reply,
        tool_calls,
    })
}

/// 待处理的工具调用（用于累积参数）
struct PendingToolCall {
    index: u32,
    id: String,
    name: String,
    arguments: String,
}

impl PendingToolCall {
    /// 完成工具调用，解析参数
    fn finalize(self) -> Option<ToolCallRecord> {
        let arguments = if self.arguments.is_empty() {
            Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&self.arguments)
                .unwrap_or_else(|_| Value::Object(serde_json::Map::new()))
        };

        Some(ToolCallRecord {
            call_id: self.id,
            name: self.name,
            arguments,
            result: None,
            success: false,
        })
    }
}

/// 解析工具参数 JSON
pub fn parse_tool_arguments(args: Value) -> serde_json::Map<String, Value> {
    if let Value::Object(m) = args {
        m
    } else {
        serde_json::Map::new()
    }
}

/// LLM Stream 类型别名（需引入）
type LlmStream = Pin<Box<dyn futures_util::Stream<Item = Result<LlmStreamEvent, LlmError>> + Send>>;
