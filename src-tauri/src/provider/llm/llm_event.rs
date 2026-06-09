use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        /// Block 类型：text, thinking, tool
        block_type: String,
        /// 块序号（自动递增）
        order_num: i32,
    },

    // ========== Agent Plan 相关 ==========
    /// Plan 开始标记（纯事件通知，不含内容）
    ///
    /// 与 BlockStart 平级，前端收到后构造 `{ type: 'plan', data: PlanDto }` 并
    /// 按 `order_num` 插入统一内容序列，保证流式 / DB 加载两路数据结构一致。
    PlanStart {
        /// Plan 记录 ID
        plan_id: String,
        /// Block 序号（与 BlockStart 共享序号空间）
        order_num: i32,
    },
    PlanSteps {
        plan_id: String,
        /// LLM 判断理由
        reasoning: String,
        /// 执行步骤列表
        steps: Vec<crate::provider::llm::types::PlanStep>,
    },
    /// Plan 执行结果更新（Agent 循环结束后推送）
    PlanUpdate {
        /// Plan 记录 ID
        plan_id: String,
        /// 各步骤执行结果（JSON 字符串）
        step_results: Option<String>,
        /// 停止原因
        stop_reason: String,
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


