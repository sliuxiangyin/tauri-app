//! MCP 服务配置 Command 层
//!
//! 薄包装层 — 仅做参数转发和错误转换，业务逻辑在 services::mcp_service::McpService 中。

use std::sync::Arc;

use crate::services::mcp_service::McpService;
use crate::types::mcp::{McpServiceDto, ResumeResult};
use tauri::State;

/// 获取运行中的 MCP 配置列表
#[tauri::command]
pub async fn get_running_mcps(
    mcp_service: State<'_, Arc<McpService>>,
) -> Result<Vec<McpServiceDto>, String> {
    mcp_service.get_running().await
}

/// 获取所有 MCP 配置
#[tauri::command]
pub async fn get_all_mcps(
    mcp_service: State<'_, Arc<McpService>>,
) -> Result<Vec<McpServiceDto>, String> {
    mcp_service.get_all().await
}

/// 获取单个 MCP 配置
#[tauri::command]
pub async fn get_mcp(
    mcp_service: State<'_, Arc<McpService>>,
    name: String,
) -> Result<Option<McpServiceDto>, String> {
    mcp_service.get(&name).await
}

/// 创建 MCP 配置
#[tauri::command]
pub async fn create_mcp(
    mcp_service: State<'_, Arc<McpService>>,
    name: String,
    config: String,
    status: String,
) -> Result<McpServiceDto, String> {
    mcp_service.create(name, config, status).await
}

/// 更新 MCP 配置
#[tauri::command]
pub async fn update_mcp(
    mcp_service: State<'_, Arc<McpService>>,
    name: String,
    config: Option<String>,
    status: Option<String>,
) -> Result<McpServiceDto, String> {
    mcp_service.update(name, config, status).await
}

/// 删除 MCP 配置
#[tauri::command]
pub async fn delete_mcp(
    mcp_service: State<'_, Arc<McpService>>,
    name: String,
) -> Result<(), String> {
    mcp_service.delete(name).await
}

/// 切换 MCP 状态
#[tauri::command]
pub async fn toggle_mcp_status(
    mcp_service: State<'_, Arc<McpService>>,
    name: String,
) -> Result<McpServiceDto, String> {
    mcp_service.toggle(name).await
}

/// 显式连接
#[tauri::command]
pub async fn mcp_connect(
    mcp_service: State<'_, Arc<McpService>>,
    name: String,
) -> Result<McpServiceDto, String> {
    mcp_service.connect(name).await
}

/// 显式断开
#[tauri::command]
pub async fn mcp_disconnect(
    mcp_service: State<'_, Arc<McpService>>,
    name: String,
) -> Result<McpServiceDto, String> {
    mcp_service.disconnect(name).await
}

/// 恢复所有已启用服务
#[tauri::command]
pub async fn mcp_resume_all(
    mcp_service: State<'_, Arc<McpService>>,
) -> Result<Vec<ResumeResult>, String> {
    mcp_service.resume_all().await
}