//! MCP 服务管理器 - 解决异步初始化竞态问题
//!
//! 设计目标：
//! - 提供可靠的初始化屏障，确保初始化完成后再接受请求
//! - 支持优雅降级（初始化失败时仍可查询状态）
//! - 提供统一的 API 访问接口

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::db::DbState;
use crate::provider::cache::Cache;
use crate::provider::mcp_v2::McpV2Api;
use crate::services::mcp_service::init_mcp_v2_with_api;

/// MCP 服务管理器状态
enum ManagerState {
    /// 正在初始化
    Initializing,
    /// 已初始化成功
    Ready {
        api: McpV2Api,
        app_handle: tauri::AppHandle,
    },
    /// 初始化失败
    Failed(String),
}

/// MCP 服务管理器 - 封装异步初始化逻辑
///
/// 提供以下保障：
/// 1. 初始化期间阻塞请求（或返回错误）
/// 2. 初始化完成后提供稳定的 API 访问
/// 3. 失败状态可查询，便于调试
pub struct McpServiceManager {
    state: RwLock<ManagerState>,
}

impl McpServiceManager {
    /// 创建新的管理器（不自动初始化）
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ManagerState::Initializing),
        }
    }

    /// 获取 API 引用（带初始化检查）
    ///
    /// 返回：
    /// - Ok(Some((api, app_handle))) - 已初始化
    /// - Ok(None) - 正在初始化中
    /// - Err(message) - 初始化失败
    pub async fn get_api(&self) -> Result<Option<(McpV2Api, tauri::AppHandle)>, String> {
        let state = self.state.read().await;
        match &*state {
            ManagerState::Ready { api, app_handle } => {
                // McpV2Api 未实现 Clone，通过重新包装 Arc 来创建新实例
                let manager = api.manager().clone();
                Ok(Some((McpV2Api::new(manager), app_handle.clone())))
            }
            ManagerState::Initializing => Ok(None), // 正在初始化
            ManagerState::Failed(msg) => Err(msg.clone()),
        }
    }

    /// 等待初始化完成
    #[allow(dead_code)]
    pub async fn wait_ready(&self, timeout: std::time::Duration) -> Result<(), String> {
        let state = self.state.read().await;
        match &*state {
            ManagerState::Ready { .. } => Ok(()),
            ManagerState::Initializing => {
                drop(state); // 释放锁
                             // 等待信号或超时
                tokio::time::timeout(timeout, async {
                    // 这里应该有一个等待机制
                    // 简化实现：检查状态直到完成或超时
                    let mut attempts = 0;
                    while attempts < 100 {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        let state = self.state.read().await;
                        match &*state {
                            ManagerState::Ready { .. } => return Ok(()),
                            ManagerState::Failed(msg) => return Err(msg.clone()),
                            ManagerState::Initializing => {}
                        }
                        drop(state);
                        attempts += 1;
                    }
                    Err("Initialization timeout".to_string())
                })
                .await
                .map_err(|_| "Wait timeout".to_string())?
            }
            ManagerState::Failed(msg) => Err(msg.clone()),
        }
    }

    /// 执行初始化
    pub async fn initialize(&self, db_state: &DbState, cache: Arc<Cache>, app_handle: tauri::AppHandle) {
        let mut state = self.state.write().await;

        // 再次检查，避免重复初始化
        if matches!(&*state, ManagerState::Ready { .. }) {
            info!("MCP service already initialized");
            return;
        }

        info!("Starting MCP service initialization...");

        match init_mcp_v2_with_api(db_state, cache).await {
            Ok(api) => {
                // 设置 AppHandle 给 ServerManager 用于发送事件
                api.manager().set_app_handle(app_handle.clone());
                *state = ManagerState::Ready { api, app_handle };
                info!("MCP service initialized successfully");
            }
            Err(e) => {
                let msg = format!("MCP service initialization failed: {}", e);
                *state = ManagerState::Failed(msg.clone());
                error!("{}", msg);
                info!("MCP service initialization failed");
            }
        }
    }

    /// 检查是否已就绪
    #[allow(dead_code)]
    pub async fn is_ready(&self) -> bool {
        matches!(*self.state.read().await, ManagerState::Ready { .. })
    }

    /// 获取错误信息（如果初始化失败）
    #[allow(dead_code)]
    pub async fn get_error(&self) -> Option<String> {
        match &*self.state.read().await {
            ManagerState::Failed(msg) => Some(msg.clone()),
            _ => None,
        }
    }
}

impl Default for McpServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServiceManager {
    /// 创建 Arc 实例（用于在 Tauri setup 前创建并注册）
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

/// 便捷函数：创建带初始化的管理器
#[allow(dead_code)]
pub async fn create_manager(db_state: &DbState, cache: Arc<Cache>, app_handle: tauri::AppHandle) -> Arc<McpServiceManager> {
    let manager = Arc::new(McpServiceManager::new());
    let db_state = db_state.clone();
    let cache = cache.clone();

    // 后台初始化
    let mgr = manager.clone();
    tauri::async_runtime::spawn(async move {
        mgr.initialize(&db_state, cache, app_handle).await;
    });

    manager
}

/// 简化版：用于替换 lib.rs 中的 McpV2State
///
/// 提供更好的错误处理和状态查询
#[allow(dead_code)]
pub type ManagedMcpState = Arc<McpServiceManager>;
