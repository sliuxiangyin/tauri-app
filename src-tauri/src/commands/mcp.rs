//! MCP 服务配置 Command 层
//! 提供给前端的 CRUD 接口

use crate::db::DbState;
use crate::services::db::mcp::{CreateMcpPayload, McpDto, UpdateMcpPayload};
use crate::services::db::mcp as db_mcp;
use tauri::State;

/// Transport 类型常量
const TRANSPORT_STDIO: &str = "stdio";
const TRANSPORT_HTTP: &str = "http";

/// 从 config JSON 自动解析 transport 类型
fn parse_transport(config_json: &str) -> String {
    // 尝试解析为 JSON 对象
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(config_json) {
        // STDIO: 包含 command 字段
        if value.get("command").is_some() {
            return TRANSPORT_STDIO.to_string();
        }
        // HTTP: 包含 url 字段
        if let Some(url) = value.get("url").and_then(|v| v.as_str()) {
            if url.contains("sse") {
                return TRANSPORT_HTTP.to_string();
            }
            if url.contains("streamable") {
                return TRANSPORT_HTTP.to_string();
            }
            return TRANSPORT_HTTP.to_string();
        }
    }
    // 默认返回 HTTP
    TRANSPORT_HTTP.to_string()
}

/// 获取所有 MCP 配置
#[tauri::command]
pub async fn get_all_mcps(db_state: State<'_, DbState>) -> Result<Vec<McpDto>, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    db_mcp::get_all_mcps(&db).await
}

/// 获取单个 MCP 配置
#[tauri::command]
pub async fn get_mcp(db_state: State<'_, DbState>, name: String) -> Result<Option<McpDto>, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    db_mcp::get_mcp_by_name(&db, &name).await
}

/// 创建 MCP 配置
#[tauri::command]
pub async fn create_mcp(
    db_state: State<'_, DbState>,
    name: String,
    config: String,  // JSON 字符串
    status: String,
) -> Result<McpDto, String> {
    // 自动解析 transport
    let transport = parse_transport(&config);
    
    let payload = CreateMcpPayload {
        name,
        transport,
        config,
        status,
    };
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    db_mcp::create_mcp(&db, payload).await
}

/// 更新 MCP 配置
#[tauri::command]
pub async fn update_mcp(
    db_state: State<'_, DbState>,
    name: String,
    config: Option<String>,  // 可选 JSON 字符串
    status: Option<String>,
) -> Result<McpDto, String> {
    let payload = UpdateMcpPayload {
        config,
        status,
        ..Default::default()
    };
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    db_mcp::update_mcp_by_name(&db, &name, payload).await
}

/// 删除 MCP 配置
#[tauri::command]
pub async fn delete_mcp(db_state: State<'_, DbState>, name: String) -> Result<(), String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    db_mcp::delete_mcp_by_name(&db, &name).await
}

/// 切换 MCP 状态
#[tauri::command]
pub async fn toggle_mcp_status(
    db_state: State<'_, DbState>,
    name: String,
) -> Result<McpDto, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    
    let mcp = db_mcp::get_mcp_by_name(&db, &name)
        .await?
        .ok_or_else(|| format!("MCP not found: {}", name))?;
    
    let new_status = if mcp.status == "enable" { "disable" } else { "enable" };
    let payload = UpdateMcpPayload { 
        status: Some(new_status.to_string()), 
        ..Default::default() 
    };
    db_mcp::update_mcp_by_name(&db, &name, payload).await
}