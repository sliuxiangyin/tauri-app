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

/// 计划步骤 - 代表一个可执行的操作单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// 步骤序号（从 1 开始）
    pub order: u8,
    /// 步骤类型
    #[serde(default)]
    pub step_type: StepType,
    /// 工具名称（完整格式，如 "mcp__server__tool"，exploratory 时可选）
    pub tool_name: String,
    /// 工具调用参数（JSON 对象）
    pub parameters: serde_json::Value,
    /// 本步骤目标描述
    pub step_goal: String,
    /// 期望输出（用于验证步骤是否成功）
    pub expected_output: Option<String>,
    /// 依赖的前置步骤序号
    pub depends_on: Option<u8>,
}

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

impl PlanStep {
    /// 创建新的计划步骤（默认确定性类型）
    pub fn new(order: u8, tool_name: impl Into<String>, step_goal: impl Into<String>) -> Self {
        Self {
            order,
            step_type: StepType::Deterministic,
            tool_name: tool_name.into(),
            parameters: serde_json::json!({}),
            step_goal: step_goal.into(),
            expected_output: None,
            depends_on: None,
        }
    }

    /// 创建探索性步骤
    pub fn exploratory(order: u8, step_goal: impl Into<String>) -> Self {
        Self {
            order,
            step_type: StepType::Exploratory,
            tool_name: String::new(),
            parameters: serde_json::json!({}),
            step_goal: step_goal.into(),
            expected_output: None,
            depends_on: None,
        }
    }

    /// 设置期望输出
    pub fn with_expected_output(mut self, output: impl Into<String>) -> Self {
        self.expected_output = Some(output.into());
        self
    }

    /// 设置依赖的前置步骤
    pub fn with_dependency(mut self, depends_on: u8) -> Self {
        self.depends_on = Some(depends_on);
        self
    }
}

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
