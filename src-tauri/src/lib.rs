mod commands;
mod db;
mod entity;
mod migration;
mod provider;
mod services;

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
        .setup(|app: &mut tauri::App| {
            let app_handle = app.handle().clone();
            let db_state = db::DbState::new(app_handle.clone());
            app.manage(db_state.clone());
            app.manage(provider::wechat::WechatClient::new(wechat_url.clone()));
            // 启动 HTTP webhook 服务
            provider::server::start_http_server(app);

            // 初始化通用缓存管理器（单例，整个应用共享）
            let cache = Arc::new(Cache::open("./app-cache").expect("Failed to initialize cache"));
            app.manage(cache.clone());

            // 启动微信消息服务（在后台运行）
            let wechat_client = provider::wechat::WechatClient::new(wechat_url);
            let webhook_channel = app
                .state::<Arc<provider::server::WebhookChannel>>()
                .inner()
                .clone();
            let db_state_for_wechat = db_state.clone();
            tauri::async_runtime::spawn(async move {
                services::wechat_message::start_wechat_message_service(
                    app_handle.clone(),
                    db_state_for_wechat,
                    webhook_channel,
                    wechat_client,
                )
                .await;
            });

            // 注册 MCP 服务管理器（替代原来的 McpV2State）
            // 先在同步 setup 中注册空的 Manager，再在后台完成真正的初始化
            let mcp_manager = services::mcp_manager::McpServiceManager::new_arc();
            app.manage(mcp_manager.clone()); // 先注册到 Tauri 状态

            // 在后台异步初始化
            let db_state_for_mcp = db_state.clone();
            let cache_for_mcp = cache.clone();
            tauri::async_runtime::spawn(async move {
                mcp_manager
                    .initialize(&db_state_for_mcp, cache_for_mcp)
                    .await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::db::db_health_check,
            commands::llm::llm_chat_once,
            commands::llm::llm_chat_stream,
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
            commands::mcp::list_mcp_serve_configs,
            commands::mcp::create_mcp_serve_config,
            commands::mcp::update_mcp_serve_config,
            commands::mcp::delete_mcp_serve_config,
            // Chat commands
            commands::chat::get_messages,
            commands::chat::save_message,
            commands::chat::delete_message,
            commands::chat::get_sessions,
            // Wechat commands
            commands::wechat::wechat_login_stream,
            commands::wechat::wechat_login_cancel,
            commands::wechat::wechat_send_message,
            commands::wechat::wechat_get_accounts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
