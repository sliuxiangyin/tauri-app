use serde::{Deserialize, Serialize};

// 从 agent::types 导入 StepType（保持向后兼容）
pub use crate::provider::llm::agent::types::StepType;

/// 统一工具定义（OpenAI 风格 function calling）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,  // 固定 "function"
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    /// 从 MCP 工具转换为统一格式
    pub fn from_mcp(name: &str, description: Option<&str>, input_schema: serde_json::Value) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: name.to_string(),
                description: description.map(String::from),
                parameters: input_schema,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,  // JSON Schema
}

/// 解析出的函数调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// 工具调用记录（包含调用信息 + 执行结果）
///
/// 用于 process_tool_batch 返回完整工具调用数据，供调用方入库。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// 工具调用 ID
    pub call_id: String,
    /// 工具名称
    pub name: String,
    /// 工具参数
    pub arguments: serde_json::Value,
    /// 工具执行结果
    pub result: Option<serde_json::Value>,
    /// 是否执行成功
    pub success: bool,
}

impl From<FunctionCall> for ToolCallRecord {
    fn from(call: FunctionCall) -> Self {
        Self {
            call_id: call.id,
            name: call.name,
            arguments: call.arguments,
            result: None,
            success: false,
        }
    }
}

impl From<ToolCallRecord> for FunctionCall {
    fn from(record: ToolCallRecord) -> Self {
        Self {
            id: record.call_id,
            name: record.name,
            arguments: record.arguments,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// 工具执行结果消息
    Tool,
}

impl Role {
    /// 从字符串转换为 Role
    pub fn from_str(s: &str) -> Self {
        match s {
            "system" => Role::System,
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            _ => Role::User,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// 工具调用 ID（仅 role=tool 时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 工具名称（仅 role=tool 时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 工具调用列表（仅 role=assistant 时可能包含）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallItem>>,
}

/// 工具调用项（用于 assistant 消息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallItem {
    pub id: String,
    pub name: String,
    /// JSON 字符串格式的参数
    pub arguments: String,
}

impl ChatMessage {
    /// 创建普通消息
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// 创建工具结果消息
    #[allow(dead_code)]
    pub fn tool_result(call_id: impl Into<String>, name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(call_id.into()),
            name: Some(name.into()),
            tool_calls: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// 工具定义（统一格式），用于 function calling
    #[serde(default)]
    pub tools: Option<Vec<ToolDefinition>>,
}

fn default_temperature() -> f32 {
    1.0
}

/// 计划步骤 - 代表一个可执行的宏观意图单元
///
/// 描述高层目标（如"完成搜索"、"提取结果"），具体操作由 ReAct 执行时动态生成 SubAction
///
/// > 结构体定义已迁移到 [`crate::provider::llm::prompts::plans_prompt`] 模块，
/// > 此处通过 `pub use` 重新导出以保持向后兼容。
pub use crate::provider::llm::prompts::plans_prompt::PlanStep;

/// 子动作 - ReAct 模式下的具体操作单元
///
/// 由 LLM 在执行时动态决定并执行，记录到 actions 列表中供后续步骤参考
///
/// > 结构体定义已迁移到 [`crate::provider::llm::prompts::plans_prompt`] 模块，
/// > 此处通过 `pub use` 重新导出以保持向后兼容。
pub use crate::provider::llm::prompts::plans_prompt::SubAction;

/// 意图计划 - LLM 识别后的执行计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentPlan {
    /// 是否需要 Agent 模式执行
    pub need_agent: bool,
    /// LLM 的判断理由
    pub reasoning: String,
    /// 执行步骤列表
    pub steps: Vec<PlanStep>,
}

#[allow(dead_code)]
impl IntentPlan {
    /// 创建一个简单的非 Agent 计划
    pub fn simple() -> Self {
        Self {
            need_agent: false,
            reasoning: "简单对话，无需工具调用".to_string(),
            steps: Vec::new(),
        }
    }

    /// 创建一个 Agent 计划
    pub fn agent(steps: Vec<PlanStep>, reasoning: impl Into<String>) -> Self {
        Self {
            need_agent: true,
            reasoning: reasoning.into(),
            steps,
        }
    }
}

// 注意：PlanStep / SubAction 结构体及其方法已迁移到 provider::llm::prompts::plans_prompt
// 此处通过 pub use 重新导出以保持向后兼容（`use crate::provider::llm::types::PlanStep` 仍然有效）

/// Payload from the frontend to select a provider and credentials (per request).
///
/// Tauri IPC 对枚举标签使用 `open_ai`（与纯 `serde` 的 `openai_compatible` 不同），
/// 故对 `kind` 与嵌套字段增加 `rename` / `alias`，便于 `invoke` 与手写 JSON 互通。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderConfigPayload {
    #[serde(rename = "open_ai")]
    OpenAiCompatible {
        #[serde(alias = "baseUrl")]
        base_url: String,
        #[serde(alias = "apiKey")]
        api_key: String,
    },
    Anthropic {
        #[serde(alias = "apiKey")]
        api_key: String,
    },
    Ollama {
        #[serde(alias = "baseUrl")]
        base_url: String,
    },
}

// 注意：StepType、StepAction、LlmDecision 已移动到 provider::llm::agent::types
// PlanStep 和 IntentPlan 保留在此文件，因为它们被多个模块引用

// ──────────────────────────────────────────────────────────────
// Block 类型定义
// ──────────────────────────────────────────────────────────────

/// 内容块类型枚举
///
/// 用于区分 conversation 表中不同类型的 block：
/// - Text: 普通文本内容
/// - Thinking: 思考过程/推理链
/// - Tool: 工具调用（包含输入参数和执行结果）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    /// 普通文本内容
    Text,
    /// 思考过程/推理链
    Thinking,
    /// 工具调用（统一类型，包含参数和结果）
    Tool,
}

impl BlockType {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockType::Text => "text",
            BlockType::Thinking => "thinking",
            BlockType::Tool => "tool",
        }
    }

    /// 从字符串转换
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "text" => BlockType::Text,
            "thinking" => BlockType::Thinking,
            "tool" => BlockType::Tool,
            _ => BlockType::Text,
        }
    }
}

impl serde::Serialize for BlockType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for BlockType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(BlockType::from_str(&s))
    }
}
