//! MCP 服务编排层
//!
//! 职责：
//! - 协调 DB 持久化与 McpManager 运行时操作
//! - create/update/delete/toggle 操作后同步更新 DB 运行状态
//! - 提供显式 connect / disconnect / resume_all 运行时控制
//! - 启动时仅重置 operating 状态，不自动连接
//!
//! 设计原则：纯被动式 — 所有连接由前端显式触发。

use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::provider::mcp::{McpManager, TransportConfig};
use crate::services::db::mcp::{self as db_mcp, CreateMcpPayload, UpdateMcpPayload};
use crate::services::traits::DbAccessor;
use crate::types::mcp::{McpServiceDto, ResumeResult};

// ──────────────────────────────────────────────────────────────
// McpService（面向对象 + 依赖注入）
// ──────────────────────────────────────────────────────────────

/// MCP 服务
///
/// 通过构造器注入 DbAccessor 和 McpManager 依赖
pub struct McpService {
    db: Arc<dyn DbAccessor>,
    mcp: Arc<McpManager>,
}

impl McpService {
    /// 创建新的 MCP 服务
    pub fn new(db: Arc<dyn DbAccessor>, mcp: Arc<McpManager>) -> Self {
        Self { db, mcp }
    }

    /// 获取运行中的 MCP 配置列表
    pub async fn get_running(&self) -> Result<Vec<McpServiceDto>, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        get_running_mcps_impl(&db, &self.mcp).await
    }

    /// 获取所有 MCP 配置（含运行时状态）
    pub async fn get_all(&self) -> Result<Vec<McpServiceDto>, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        get_all_mcps_impl(&db, &self.mcp).await
    }

    /// 获取单个 MCP 配置
    pub async fn get(&self, name: &str) -> Result<Option<McpServiceDto>, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        get_mcp_impl(&db, &self.mcp, name).await
    }

    /// 创建 MCP 配置
    pub async fn create(&self, name: String, config: String, status: String) -> Result<McpServiceDto, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        create_mcp_impl(db, Arc::clone(&self.mcp), name, config, status).await
    }

    /// 更新 MCP 配置
    pub async fn update(&self, name: String, config: Option<String>, status: Option<String>) -> Result<McpServiceDto, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        update_mcp_impl(db, Arc::clone(&self.mcp), name, config, status).await
    }

    /// 删除 MCP 配置
    pub async fn delete(&self, name: String) -> Result<(), String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        delete_mcp_impl(&db, &self.mcp, name).await
    }

    /// 切换 MCP 状态
    pub async fn toggle(&self, name: String) -> Result<McpServiceDto, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        toggle_mcp_status_impl(db, Arc::clone(&self.mcp), name).await
    }

    /// 显式连接
    pub async fn connect(&self, name: String) -> Result<McpServiceDto, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        connect_mcp_impl(&db, &self.mcp, name).await
    }

    /// 显式断开
    pub async fn disconnect(&self, name: String) -> Result<McpServiceDto, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        disconnect_mcp_impl(&db, &self.mcp, name).await
    }

    /// 恢复所有已启用的服务
    pub async fn resume_all(&self) -> Result<Vec<ResumeResult>, String> {
        let db = self.db.get().await.map_err(|e| e.to_string())?;
        resume_all_enabled_impl(&db, &self.mcp).await
    }

    /// 启动时重置所有 operating 状态
    pub async fn reset_on_startup(&self) {
        let db = self.db.get().await.map_err(|e| e.to_string());
        if let Ok(db) = db {
            reset_all_operating_on_startup(&db).await;
        }
    }
}

// ──────────────────────────────────────────────────────────────
// 配置解析
// ──────────────────────────────────────────────────────────────

// ─── 配置解析 ─────────────────────────────────────────────

/// 从 DB config JSON 解析为 TransportConfig
fn parse_transport_config(config_json: &str) -> Result<TransportConfig, String> {
    let value: serde_json::Value =
        serde_json::from_str(config_json).map_err(|e| format!("Invalid config JSON: {}", e))?;

    if let Some(command) = value.get("command").and_then(|v| v.as_str()) {
        let args: Vec<String> = value
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|a| a.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let env: HashMap<String, String> = value
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        return Ok(TransportConfig::Stdio { command: command.to_string(), args, env });
    }

    if let Some(url) = value.get("url").and_then(|v| v.as_str()) {
        return Ok(TransportConfig::Http { url: url.to_string() });
    }

    Err("config must contain 'command' (stdio) or 'url' (http) field".to_string())
}

// ─── 运行时结果写入 DB ────────────────────────────────────

/// 连接成功后获取 tools 并写入 DB
async fn after_connect_success(
    db: &DatabaseConnection,
    mcp: &McpManager,
    name: &str,
) {
    // 尝试获取 tools
    match mcp.get_tools(name).await {
        Ok(tools) => {
            let tools_json = serde_json::to_string(&tools).unwrap_or_default();
            let payload = UpdateMcpPayload {
                operating: Some("running".to_string()),
                tools: Some(tools_json),
                error_msg: Some(String::new()),
                ..Default::default()
            };
            if let Err(e) = db_mcp::update_mcp_by_name(db, name, payload).await {
                warn!("[McpService] failed to update DB after connect '{}': {}", name, e);
            } else {
                info!("[McpService] '{}' connected, {} tools found", name, tools.len());
            }
        }
        Err(e) => {
            // 连接成功但 tools 获取失败 — 仍标记 running，tools 留空
            warn!("[McpService] '{}' connected but failed to list tools: {}", name, e);
            let payload = UpdateMcpPayload {
                operating: Some("running".to_string()),
                error_msg: Some(String::new()),
                ..Default::default()
            };
            let _ = db_mcp::update_mcp_by_name(db, name, payload).await;
        }
    }
}

/// 连接失败后写入 DB 错误信息
async fn after_connect_failure(
    db: &DatabaseConnection,
    name: &str,
    error: &str,
) {
    let payload = UpdateMcpPayload {
        operating: Some("failed".to_string()),
        error_msg: Some(error.to_string()),
        ..Default::default()
    };
    if let Err(e) = db_mcp::update_mcp_by_name(db, name, payload).await {
        warn!("[McpService] failed to update DB after connect failure '{}': {}", name, e);
    }
}

// ─── 异步连接任务 ─────────────────────────────────────────────

/// 异步执行 MCP 连接并写入结果
///
/// 封装为独立任务，避免阻塞请求处理流程。
/// 连接完成后根据结果调用 after_connect_success 或 after_connect_failure。
///
/// 注意：此函数内部 clone 了 db 和 mcp，确保任务独立持有这些资源。
pub fn spawn_connect_task(
    db: Arc<DatabaseConnection>,
    mcp: Arc<McpManager>,
    name: String,
    transport: TransportConfig,
) -> JoinHandle<()> {
    let name_clone = name.clone();
    tokio::spawn(async move {
        info!("[McpService] spawn_connect_task: '{}' starting in background", name_clone);
        match mcp.connect(&name_clone, transport).await {
            Ok(_) => {
                after_connect_success(&db, &mcp, &name_clone).await;
                info!("[McpService] spawn_connect_task: '{}' succeeded", name_clone);
            }
            Err(e) => {
                let err_str = e.to_string();
                after_connect_failure(&db, &name_clone, &err_str).await;
                warn!("[McpService] spawn_connect_task: '{}' failed: {}", name_clone, err_str);
            }
        }
    })
}

/// 异步执行 MCP 重启并写入结果
pub fn spawn_restart_task(
    db: Arc<DatabaseConnection>,
    mcp: Arc<McpManager>,
    name: String,
    transport: TransportConfig,
) -> JoinHandle<()> {
    let name_clone = name.clone();
    tokio::spawn(async move {
        info!("[McpService] spawn_restart_task: '{}' starting in background", name_clone);
        match mcp.restart(&name_clone, transport).await {
            Ok(_) => {
                after_connect_success(&db, &mcp, &name_clone).await;
                info!("[McpService] spawn_restart_task: '{}' succeeded", name_clone);
            }
            Err(e) => {
                let err_str = e.to_string();
                after_connect_failure(&db, &name_clone, &err_str).await;
                warn!("[McpService] spawn_restart_task: '{}' failed: {}", name_clone, err_str);
            }
        }
    })
}

/// 断开后写入 DB
async fn after_disconnect(db: &DatabaseConnection, name: &str) {
    let payload = UpdateMcpPayload {
        operating: Some("idle".to_string()),
        tools: Some(String::new()),
        error_msg: Some(String::new()),
        ..Default::default()
    };
    if let Err(e) = db_mcp::update_mcp_by_name(db, name, payload).await {
        warn!("[McpService] failed to update DB after disconnect '{}': {}", name, e);
    }
}

/// 连接开始前写入 DB（operating=connecting，前端可展示加载态）
async fn set_operating_connecting(db: &DatabaseConnection, name: &str) {
    let payload = UpdateMcpPayload {
        operating: Some("connecting".to_string()),
        ..Default::default()
    };
    let _ = db_mcp::update_mcp_by_name(db, name, payload).await;
}

// ─── 公开 API ─────────────────────────────────────────────

/// 获取运行中的 MCP 配置列表（内部实现）
async fn get_running_mcps_impl(
    db: &DatabaseConnection,
    mcp: &McpManager,
) -> Result<Vec<McpServiceDto>, String> {
    let list = db_mcp::get_all_mcps(db).await?;
    Ok(list
        .into_iter()
        .filter(|dto| dto.status == "enable" && dto.operating == "running")
        .map(|dto| {
            let runtime = mcp.get_status(&dto.name);
            McpServiceDto::from_db_and_runtime(dto, runtime.as_ref())
        })
        .collect())
}

/// 获取所有 MCP 配置（内部实现）
async fn get_all_mcps_impl(
    db: &DatabaseConnection,
    mcp: &McpManager,
) -> Result<Vec<McpServiceDto>, String> {
    let list = db_mcp::get_all_mcps(db).await?;
    Ok(list
        .into_iter()
        .map(|dto| {
            let runtime = mcp.get_status(&dto.name);
            McpServiceDto::from_db_and_runtime(dto, runtime.as_ref())
        })
        .collect())
}

/// 获取单个 MCP 配置（内部实现）
async fn get_mcp_impl(
    db: &DatabaseConnection,
    mcp: &McpManager,
    name: &str,
) -> Result<Option<McpServiceDto>, String> {
    let dto = db_mcp::get_mcp_by_name(db, name).await?;
    Ok(dto.map(|dto| {
        let runtime = mcp.get_status(&dto.name);
        McpServiceDto::from_db_and_runtime(dto, runtime.as_ref())
    }))
}

/// 创建 MCP 配置（内部实现）
async fn create_mcp_impl(
    db: Arc<DatabaseConnection>,
    mcp: Arc<McpManager>,
    name: String,
    config: String,
    status: String,
) -> Result<McpServiceDto, String> {
    let transport = parse_transport_config(&config)?;
    let is_enable = status == "enable";

    // 1. 写入 DB
    let payload = CreateMcpPayload {
        name: name.clone(),
        transport: transport.transport_type().to_string(),
        config,
        status,
    };
    let dto = db_mcp::create_mcp(&db, payload).await?;

    // 2. 若 enable，异步执行连接（optimistic response）
    if is_enable {
        set_operating_connecting(&db, &dto.name).await;
        spawn_connect_task(Arc::clone(&db), Arc::clone(&mcp), dto.name.clone(), transport);
    }

    // 3. 返回 optimistic 状态
    get_mcp_impl(&db, &mcp, &dto.name)
        .await
        .map(|opt| opt.expect("just created MCP must exist"))
}

/// 更新 MCP 配置（内部实现）
async fn update_mcp_impl(
    db: Arc<DatabaseConnection>,
    mcp: Arc<McpManager>,
    name: String,
    config: Option<String>,
    status: Option<String>,
) -> Result<McpServiceDto, String> {
    // 1. 读旧配置
    let old = db_mcp::get_mcp_by_name(&db, &name)
        .await?
        .ok_or_else(|| format!("MCP not found: {}", name))?;

    let new_config = config.clone();
    let new_status = status.clone();

    // 2. 写 DB
    let update_payload = UpdateMcpPayload {
        config: config.clone(),
        status: status.clone(),
        ..Default::default()
    };
    db_mcp::update_mcp_by_name(&db, &name, update_payload).await?;

    // 3. 判断是否需要运行时操作（异步执行）
    let config_changed = config.is_some() && config.as_deref() != Some(&old.config);
    let status_changed = status.is_some() && status.as_deref() != Some(&old.status);
    let effective_status = new_status.as_deref().unwrap_or(&old.status);

    if config_changed {
        // config 变了 → restart
        if effective_status == "enable" {
            let transport = parse_transport_config(
                new_config.as_deref().unwrap_or(&old.config),
            )?;
            set_operating_connecting(&db, &name).await;
            spawn_restart_task(Arc::clone(&db), Arc::clone(&mcp), name.clone(), transport);
        } else {
            // config 变了但 status=disable → 只断开
            let _ = mcp.disconnect(&name).await;
            after_disconnect(&db, &name).await;
        }
    } else if status_changed {
        // 仅 status 变了
        if effective_status == "enable" {
            let transport = parse_transport_config(&old.config)?;
            set_operating_connecting(&db, &name).await;
            spawn_connect_task(Arc::clone(&db), Arc::clone(&mcp), name.clone(), transport);
        } else {
            let _ = mcp.disconnect(&name).await;
            after_disconnect(&db, &name).await;
        }
    }

    // 4. 返回 optimistic 状态
    get_mcp_impl(&db, &mcp, &name)
        .await
        .map(|opt| opt.expect("just updated MCP must exist"))
}

/// 删除 MCP 配置（内部实现）
async fn delete_mcp_impl(
    db: &DatabaseConnection,
    mcp: &McpManager,
    name: String,
) -> Result<(), String> {
    // 1. 断开运行时（容错 NotFound）
    let _ = mcp.remove_from_pool(&name).await;

    // 2. 删除 DB
    db_mcp::delete_mcp_by_name(db, &name).await?;

    info!("[McpService] deleted MCP '{}'", name);
    Ok(())
}

/// 切换 MCP 状态（内部实现）
async fn toggle_mcp_status_impl(
    db: Arc<DatabaseConnection>,
    mcp: Arc<McpManager>,
    name: String,
) -> Result<McpServiceDto, String> {
    // 1. 读当前配置
    let current = db_mcp::get_mcp_by_name(&db, &name)
        .await?
        .ok_or_else(|| format!("MCP not found: {}", name))?;

    let new_status = if current.status == "enable" {
        "disable"
    } else {
        "enable"
    };

    // 2. 更新 DB status
    let update_payload = UpdateMcpPayload {
        status: Some(new_status.to_string()),
        ..Default::default()
    };
    db_mcp::update_mcp_by_name(&db, &name, update_payload).await?;

    // 3. 运行时操作（enable 时异步连接）
    if new_status == "enable" {
        let transport = parse_transport_config(&current.config)?;
        set_operating_connecting(&db, &name).await;
        spawn_connect_task(Arc::clone(&db), Arc::clone(&mcp), name.clone(), transport);
    } else {
        let _ = mcp.disconnect(&name).await;
        after_disconnect(&db, &name).await;
    }

    // 4. 返回 optimistic 状态
    get_mcp_impl(&db, &mcp, &name)
        .await
        .map(|opt| opt.expect("just toggled MCP must exist"))
}

/// 显式连接（内部实现）
async fn connect_mcp_impl(
    db: &DatabaseConnection,
    mcp: &McpManager,
    name: String,
) -> Result<McpServiceDto, String> {
    let config = db_mcp::get_mcp_by_name(db, &name)
        .await?
        .ok_or_else(|| format!("MCP not found: {}", name))?;

    info!("[McpService] explicitly connecting '{}'", name);
    set_operating_connecting(db, &name).await;

    let transport = parse_transport_config(&config.config)?;
    match mcp.connect(&name, transport).await {
        Ok(_) => after_connect_success(db, mcp, &name).await,
        Err(e) => after_connect_failure(db, &name, &e.to_string()).await,
    }

    get_mcp_impl(db, mcp, &name)
        .await
        .map(|opt| opt.expect("just connected MCP must exist"))
}

/// 显式断开（内部实现）
async fn disconnect_mcp_impl(
    db: &DatabaseConnection,
    mcp: &McpManager,
    name: String,
) -> Result<McpServiceDto, String> {
    // 确认配置存在
    db_mcp::get_mcp_by_name(db, &name)
        .await?
        .ok_or_else(|| format!("MCP not found: {}", name))?;

    info!("[McpService] explicitly disconnecting '{}'", name);
    let _ = mcp.disconnect(&name).await;
    after_disconnect(db, &name).await;

    get_mcp_impl(db, mcp, &name)
        .await
        .map(|opt| opt.expect("just disconnected MCP must exist"))
}

/// 恢复所有已启用服务（内部实现）
async fn resume_all_enabled_impl(
    db: &DatabaseConnection,
    mcp: &McpManager,
) -> Result<Vec<ResumeResult>, String> {
    let list = db_mcp::get_all_mcps(db).await?;

    let targets: Vec<_> = list
        .into_iter()
        .filter(|m| m.status == "enable" && m.operating != "running")
        .collect();

    if targets.is_empty() {
        info!("[McpService] resume_all: nothing to resume");
        return Ok(vec![]);
    }

    info!(
        "[McpService] resume_all: connecting {} service(s)...",
        targets.len()
    );

    let mut results = Vec::with_capacity(targets.len());

    for item in targets {
        set_operating_connecting(db, &item.name).await;

        let transport = match parse_transport_config(&item.config) {
            Ok(t) => t,
            Err(e) => {
                warn!("[McpService] resume_all: invalid config for '{}': {}", item.name, e);
                after_connect_failure(db, &item.name, &e).await;
                results.push(ResumeResult {
                    name: item.name,
                    success: false,
                    operating: "failed".to_string(),
                    error_msg: Some(e),
                });
                continue;
            }
        };

        match mcp.connect(&item.name, transport).await {
            Ok(_) => {
                after_connect_success(db, mcp, &item.name).await;
                results.push(ResumeResult {
                    name: item.name.clone(),
                    success: true,
                    operating: "running".to_string(),
                    error_msg: None,
                });
                info!("[McpService] resume_all: '{}' OK", item.name);
            }
            Err(e) => {
                let err_str = e.to_string();
                after_connect_failure(db, &item.name, &err_str).await;
                results.push(ResumeResult {
                    name: item.name,
                    success: false,
                    operating: "failed".to_string(),
                    error_msg: Some(err_str),
                });
            }
        }
    }

    info!(
        "[McpService] resume_all: complete, {}/{} succeeded",
        results.iter().filter(|r| r.success).count(),
        results.len()
    );
    Ok(results)
}

/// 启动时重置所有 MCP 的 operating 为 idle
///
/// 原因：应用重启后所有之前的运行时连接都已失效，
/// 必须重置 DB 中的 operating 状态，避免与 runtime_health=none 矛盾。
pub async fn reset_all_operating_on_startup(db: &DatabaseConnection) {
    let list = match db_mcp::get_all_mcps(db).await {
        Ok(list) => list,
        Err(e) => {
            error!("[McpService] startup reset: failed to list MCP configs: {}", e);
            return;
        }
    };

    for item in list {
        if item.operating != "idle" {
            let payload = UpdateMcpPayload {
                operating: Some("idle".to_string()),
                tools: Some(String::new()),
                error_msg: Some(String::new()),
                ..Default::default()
            };
            if let Err(e) = db_mcp::update_mcp_by_name(db, &item.name, payload).await {
                warn!(
                    "[McpService] startup reset: failed to reset '{}': {}",
                    item.name, e
                );
            }
        }
    }

    info!("[McpService] startup reset: complete");
}
