//! 普通（Ordinary）LLM 流处理模块
//!
//! 包含 `process_tool_batch` 及其辅助类型/函数，
//! 负责在 LLM 流式响应中累积工具调用、执行并返回完整结果。

use futures_util::StreamExt;
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use crate::provider::llm::llm_tool_trait::ToolExecutor;
use crate::provider::llm::block_sender::BlockSender;
use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::types::ToolCallRecord;

/// LLM Stream 类型别名
pub type LlmStream =
    Pin<Box<dyn futures_util::Stream<Item = Result<LlmStreamEvent, LlmError>> + Send>>;

// =============================================================================
// 工具执行上下文
// =============================================================================

/// 工具执行上下文
///
/// 用于在流式处理中追踪工具执行状态。
#[allow(dead_code)]
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

#[allow(dead_code)]
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

// =============================================================================
// 流式处理结果
// =============================================================================

/// 流式处理最终结果
///
/// 包含 LLM 回复文本和完整的工具调用记录（含执行结果）。
/// 供调用方统一入库使用。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessResult {
    /// 最终文本回复
    pub text: String,
    /// 工具调用记录列表（包含调用信息 + 执行结果）
    pub tool_calls: Vec<ToolCallRecord>,
}

// =============================================================================
// process_tool_batch
// =============================================================================

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
                        block_sender.send("tool");
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
                    LlmStreamEvent::ToolCallDone { index: idx, arguments } => {
                        // 忽略 ToolCallDone 中的 arguments（可能是空对象）
                        // 实际参数已通过 ToolCallDelta 累积到 current_call.arguments
                        // 标记参数已完整
                        if let Some(ref mut tc) = current_call {
                            // 将累积的参数字符串解析为 JSON Value，然后重新转为字符串
                            // 确保参数格式正确
                            if !tc.arguments.is_empty() {
                                if let Ok(parsed) = serde_json::from_str::<Value>(&tc.arguments) {
                                    tc.arguments = parsed.to_string();
                                }
                            }
                        }
                        // 转发 ToolCallDone 给前端
                        if let Some(ref s) = sender {
                            let _ = s.send(LlmStreamEvent::ToolCallDone {
                                index: idx,
                                arguments,
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
    block_sender.send("tool");  // 统一为 tool 类型
    if let Some(ref exec) = executor {
        for mut call in pending_calls {
            // 执行工具
            match exec.execute_tool(call.clone().into()).await {
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

// =============================================================================
// 辅助类型与函数
// =============================================================================

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
