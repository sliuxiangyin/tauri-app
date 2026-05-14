mod commands;
mod db;
mod entity;
mod migration;
mod provider;
mod server;

use std::sync::Arc;
use tauri::Manager;

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
            app.manage(provider::mcp::McpManager::new());
            app.manage(provider::wechat::WechatClient::new(wechat_url));

            // 启动 HTTP webhook 服务
            server::start_http_server(app);

            // 在后台启动 webhook 消息监听任务（订阅 broadcast 通道）
            // 前端只需通过 listen('wechat:message') 接收事件，无需调用 command
            let channel = app.state::<Arc<crate::server::WebhookChannel>>();
            commands::wechat::wechat_listen_messages(app_handle.clone(), channel.inner().clone());

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
            // MCP commands
            commands::mcp::mcp_connect,
            commands::mcp::mcp_disconnect,
            commands::mcp::mcp_list_tools,
            commands::mcp::mcp_call_tool,
            commands::mcp::mcp_list_services,
            commands::mcp::mcp_is_service_connected,
            commands::mcp::mcp_clear_tools_cache,
            // Wechat commands
            commands::wechat::wechat_login_stream,
            commands::wechat::wechat_send_message,
            commands::wechat::wechat_get_accounts,
            
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
