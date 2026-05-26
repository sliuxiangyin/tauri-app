//! MCP 服务相关类型定义
//!
//! 包含：
//! - 前端 DTO：McpServiceDto（DB 配置 + 运行时状态合并）

use serde::{Deserialize, Serialize};

use crate::provider::mcp::McpStatus;
use crate::services::db::mcp::McpDto;

/// 返回给前端的 MCP 服务 DTO（DB 配置 + 运行时状态）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServiceDto {
    // DB 字段
    pub id: i32,
    pub name: String,
    pub transport: String,
    pub config: String,
    pub status: String,
    pub operating: String,
    pub tools: Option<String>,
    pub error_msg: Option<String>,
    pub updated_at: String,
    // 运行时字段
    pub runtime_health: Option<String>,
    pub runtime_circuit_open: Option<bool>,
}

impl McpServiceDto {
    /// 从 DB DTO + 运行时状态合并
    pub fn from_db_and_runtime(db: McpDto, runtime: Option<&McpStatus>) -> Self {
        Self {
            id: db.id,
            name: db.name,
            transport: db.transport,
            config: db.config,
            status: db.status,
            operating: db.operating,
            tools: db.tools,
            error_msg: db.error_msg,
            updated_at: db.updated_at,
            runtime_health: runtime.map(|s| s.health.clone()),
            runtime_circuit_open: runtime.map(|s| s.circuit_open),
        }
    }
}

/// resume_all 单服务结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeResult {
    pub name: String,
    pub success: bool,
    pub operating: String,
    pub error_msg: Option<String>,
}
