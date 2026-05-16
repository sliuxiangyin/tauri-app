use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::provider::mcp1111::McpError;

/// MCP 服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport")]
pub enum McpModelConfig {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    #[serde(rename = "http")]
    Http { url: String },
}

/// MCP 服务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServiceInfo {
    pub service_id: String,
    pub name: Option<String>,
    pub config: McpModelConfig,
    pub connected: bool,
    pub last_connected_at: Option<u64>,
}

/// 工具信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

/// 工具列表缓存
#[derive(Debug, Clone)]
pub struct CachedToolsList {
    pub tools: Vec<ToolInfo>,
    pub cached_at: u64,
    pub ttl_seconds: u64,
}

impl CachedToolsList {
    pub fn new(tools: Vec<ToolInfo>, ttl_seconds: u64) -> Self {
        let cached_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            tools,
            cached_at,
            ttl_seconds,
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.cached_at > self.ttl_seconds
    }
}

/// 工具执行请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
}

/// 工具内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContent {
    pub r#type: String, // "text", "image", "resource", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}
/// mcp 服务状态结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStateResult {
    pub id: String,
    pub name: String,
    pub state: bool,
    pub tools: Vec<ToolInfo>,
    //失败原因
    pub error: Option<String>,
}