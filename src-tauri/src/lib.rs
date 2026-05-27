mod commands;
mod db;
mod entity;
mod migration;
mod provider;
mod services;
mod types;

use crate::provider::cache::Cache;
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化 tracing 日志系统
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("tauri_app=debug".parse().unwrap()))
        .init();

    let wechat_url = "http://localhost:8080".to_string();
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(move |app: &mut tauri::App| {
            let app_handle = app.handle().clone();
            let db_state = db::DbState::new(app_handle.clone());
            app.manage(db_state.clone());

            // 启动 HTTP webhook 服务（内部会创建 WechatClient）
            provider::server::start_http_server(app);

            // 初始化通用缓存管理器（单例，整个应用共享）
            let cache = Cache::open("./app-cache").expect("Failed to initialize cache");
            // 设置全局单例并获取 Arc 包装
            let cache_arc = Cache::set_global(cache).expect("Failed to set Cache global");
            // 注册到 Tauri State（供需要 State 的命令使用）
            app.manage(cache_arc.clone());

            // 初始化 LLM abort flags 管理器
            app.manage(commands::llm::LlmAbortFlags::new());

            // 启动微信消息服务（在后台运行）
            let wechat_client = provider::wechat::WechatClient::new(wechat_url.clone());
            app.manage(wechat_client.clone());

            let webhook_channel = app
                .state::<Arc<provider::server::WebhookChannel>>()
                .inner()
                .clone();
            let db_state_for_wechat = db_state.clone();
            let cache_for_wechat = cache_arc.clone();
            tauri::async_runtime::spawn(async move {
                services::wechat_message::start_wechat_message_service(
                    app_handle.clone(),
                    db_state_for_wechat,
                    webhook_channel,
                    wechat_client,
                    cache_for_wechat,
                )
                .await;
            });

            // 注册 MCP 运行时管理器（纯运行时，不依赖 DB）
            let mcp_manager = Arc::new(provider::mcp::McpManager::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .build()
                    .expect("Failed to build reqwest client for MCP"),
            ));
            app.manage(mcp_manager.clone());

            // 启动时重置 operating 为 idle（上次运行时的连接已失效）
            tauri::async_runtime::spawn(async move {
                let db = match db_state.get().await {
                    Ok(db) => db,
                    Err(e) => {
                        tracing::error!("[McpService] startup reset: failed to get DB: {}", e);
                        return;
                    }
                };
                services::mcp_service::reset_all_operating_on_startup(&db).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::db::db_health_check,
            commands::llm::llm_chat_once,
            commands::llm::llm_chat_stream,
            commands::llm::llm_chat_cancel,
            commands::model_config::list_provider_configs,
            commands::model_config::create_provider_config,
            commands::model_config::update_provider_config,
            commands::model_config::delete_provider_config,
            commands::model_config::upsert_provider_model,
            commands::model_config::delete_provider_model,
            commands::model_config::reorder_provider_configs,
            commands::model_config::reorder_provider_models,
            commands::model_config::resolve_provider_payload,
            // MCP serve config commands
            commands::mcp::get_running_mcps,
            commands::mcp::get_all_mcps,
            commands::mcp::get_mcp,
            commands::mcp::create_mcp,
            commands::mcp::update_mcp,
            commands::mcp::delete_mcp,
            commands::mcp::toggle_mcp_status,
            // MCP runtime control commands (方案 B: 纯被动式)
            commands::mcp::mcp_connect,
            commands::mcp::mcp_disconnect,
            commands::mcp::mcp_resume_all,
            // Chat commands
            commands::chat::get_messages,
            commands::chat::clear_messages,
            commands::chat::get_sessions,
            // Chat model commands
            commands::chat_model::set_chat_model,
            commands::chat_model::get_chat_model,
            commands::chat_model::get_all_chat_models,
            // Wechat commands
            commands::wechat::wechat_login_stream,
            commands::wechat::wechat_login_cancel,
            commands::wechat::wechat_send_message,
            commands::wechat::wechat_get_accounts,
            // Chat tools commands
            commands::chat_tools::get_chat_tools_config,
            commands::chat_tools::save_chat_tools_config,
            commands::chat_tools::delete_chat_tools_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
