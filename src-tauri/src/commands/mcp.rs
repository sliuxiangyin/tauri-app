//! MCP 服务配置 Command 层
//!
//! 薄包装层 — 仅做参数转发和错误转换，业务逻辑在 services::mcp_service 中。

use std::sync::Arc;

use crate::db::DbState;
use crate::provider::mcp::McpManager;
use crate::services::mcp_service;
use crate::types::mcp::{McpServiceDto, ResumeResult};
use tauri::State;

/// 获取运行中的 MCP 配置列表（status=enable 且 operating=running）
#[tauri::command]
pub async fn get_running_mcps(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
) -> Result<Vec<McpServiceDto>, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::get_running_mcps(&db, &mcp_manager).await
}

/// 获取所有 MCP 配置（含运行时状态）
#[tauri::command]
pub async fn get_all_mcps(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
) -> Result<Vec<McpServiceDto>, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::get_all_mcps(&db, &mcp_manager).await
}

/// 获取单个 MCP 配置（含运行时状态）
#[tauri::command]
pub async fn get_mcp(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    name: String,
) -> Result<Option<McpServiceDto>, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::get_mcp(&db, &mcp_manager, &name).await
}

/// 创建 MCP 配置（自动解析 transport + 若 enable 则异步连接）
#[tauri::command]
pub async fn create_mcp(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    name: String,
    config: String,
    status: String,
) -> Result<McpServiceDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::create_mcp(db, Arc::clone(&mcp_manager), name, config, status).await
}

/// 更新 MCP 配置（若 config/status 变更则异步连接/重启）
#[tauri::command]
pub async fn update_mcp(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    name: String,
    config: Option<String>,
    status: Option<String>,
) -> Result<McpServiceDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::update_mcp(db, Arc::clone(&mcp_manager), name, config, status).await
}

/// 删除 MCP 配置（断开连接 + 删除 DB）
#[tauri::command]
pub async fn delete_mcp(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    name: String,
) -> Result<(), String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::delete_mcp(&db, &mcp_manager, name).await
}

/// 切换 MCP 状态（enable ↔ disable，自动异步连接/断开）
#[tauri::command]
pub async fn toggle_mcp_status(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    name: String,
) -> Result<McpServiceDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::toggle_mcp_status(db, Arc::clone(&mcp_manager), name).await
}

/// 显式连接一个 MCP 服务（不改变 status，纯运行时操作）
#[tauri::command]
pub async fn mcp_connect(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    name: String,
) -> Result<McpServiceDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::connect_mcp(&db, &mcp_manager, name).await
}

/// 显式断开一个 MCP 服务（不改变 status，不删 DB）
#[tauri::command]
pub async fn mcp_disconnect(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
    name: String,
) -> Result<McpServiceDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::disconnect_mcp(&db, &mcp_manager, name).await
}

/// 一键恢复所有 status=enable 且未运行的服务
#[tauri::command]
pub async fn mcp_resume_all(
    db_state: State<'_, DbState>,
    mcp_manager: State<'_, Arc<McpManager>>,
) -> Result<Vec<ResumeResult>, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    mcp_service::resume_all_enabled(&db, &mcp_manager).await
}
