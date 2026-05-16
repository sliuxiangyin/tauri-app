mod commands;
mod db;
mod entity;
mod migration;
mod provider;
mod server;
mod services;

use std::sync::Arc;
use tauri::Manager;
use crate::provider::mcp_v2::{McpV2Api, McpV2State};
use crate::provider::cache::Cache;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let wechat_url = "http://localhost:8080".to_string();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app: &mut tauri::App| {
            let app_handle = app.handle().clone();
            let db_state = db::DbState::new(app_handle.clone());
            app.manage(db_state);
            app.manage(provider::wechat::WechatClient::new(wechat_url));

            // 启动 HTTP webhook 服务
            server::start_http_server(app);

            // 在后台启动 webhook 消息监听任务（订阅 broadcast 通道）
            // 前端只需通过 listen('wechat:message') 接收事件，无需调用 command
            let channel = app.state::<Arc<crate::server::WebhookChannel>>();
            commands::wechat::wechat_listen_messages(app_handle.clone(), channel.inner().clone());

            // 初始化通用缓存管理器（单例，整个应用共享）
            let cache = Arc::new(
                Cache::open("./app-cache")
                    .expect("Failed to initialize cache"),
            );
            app.manage(cache);

            // 注册 MCP v2 状态占位（异步初始化完成后填充）
            let mcp_v2_state: McpV2State = Arc::new(tokio::sync::RwLock::new(None));
            app.manage(mcp_v2_state.clone());

            // 批量初始化所有已保存的 MCP 服务（在异步任务中执行，不阻塞启动）
            let app_handle_for_mcp = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let db_state = app_handle_for_mcp.state::<db::DbState>();
                match services::mcp_service::init_mcp_v2(db_state.inner()).await {
                    Ok(manager) => {
                        let api = McpV2Api::new(manager);
                        let mut guard = mcp_v2_state.write().await;
                        *guard = Some(api);
                        println!("MCP v2 services initialized successfully");
                    }
                    Err(e) => {
                        eprintln!("Failed to initialize MCP v2 services: {}", e);
                    }
                }
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

            
            // Wechat commands
            commands::wechat::wechat_login_stream,
            commands::wechat::wechat_send_message,
            commands::wechat::wechat_get_accounts,
            
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
