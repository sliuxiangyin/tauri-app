//! 微信消息服务层
//!
//! 职责：
//! - 监听 Webhook 通道消息
//! - 消息落库（wechat 渠道）
//! - 调用 LLM 处理微信消息
//! - 回复微信

use nanoid::nanoid;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::db::DbState;
use crate::entity::chat_message::{ActiveModel, Model as ChatMessageModel};
use crate::entity::model_provider_config as mpc;
use crate::provider::llm::{
    provider_trait::LlmProvider,
    types::{ChatMessage as LlmChatMessage, ChatRequest, ProviderConfigPayload, Role},
    Provider,
};
use crate::provider::server::WebhookChannel;
use crate::provider::wechat::{SendMessageRequest, WechatClient};

/// 生成唯一 ID
fn generate_id() -> String {
    nanoid!(21)
}

/// 启动微信消息监听服务
/// 在后台持续从 broadcast 通道接收消息，执行以下操作：
/// 1. 消息落库（chat_type='wechat', role='user'）
/// 2. 推送事件到前端
/// 3. 调用 LLM 获取回复
/// 4. LLM 回复落库（chat_type='wechat', role='assistant'）
/// 5. 将 LLM 回复发送到微信
pub async fn start_wechat_message_service(
    app: AppHandle,
    db_state: DbState,
    channel: Arc<WebhookChannel>,
    wechat_client: WechatClient,
) {
    let mut rx = channel.subscribe();

    println!("[WechatMessageService] 启动微信消息监听服务");

    while let Ok(payload) = rx.recv().await {
        let account_id = payload.account_id.clone();
        let from = payload.from.clone();
        let body = payload.body.clone();

        println!(
            "[WechatMessageService] 收到消息 from={} body={}",
            from, body
        );

        // 1. 保存微信消息到数据库
        match save_wechat_message(&db_state, &account_id, &body).await {
            Ok(msg) => {
                println!("[WechatMessageService] 消息已落库: id={}", msg.id);
            }
            Err(e) => {
                println!("[WechatMessageService] 消息落库失败: {}", e);
            }
        }

        // 2. 发送原生系统通知
        send_notification(&app, &from, &body);

        // 3. 推送给前端
        let _ = app.emit("wechat:message", &json!(payload));

        // 4. 调用 LLM 获取回复
        match call_llm(&db_state, &account_id, &body).await {
            Ok(reply) => {
                println!("[WechatMessageService] LLM 回复: {}", reply);

                // 5. 保存 LLM 回复
                if let Err(e) = save_llm_reply(&db_state, &account_id, &reply).await {
                    println!("[WechatMessageService] LLM 回复落库失败: {}", e);
                }

                // 6. 发送回复到微信
                if let Err(e) =
                    send_reply_to_wechat(&wechat_client, &account_id, &from, &reply).await
                {
                    println!("[WechatMessageService] 发送微信回复失败: {}", e);
                }

                // 7. 推送 LLM 回复到前端
                let _ = app.emit(
                    "wechat:llm_reply",
                    &json!({
                        "account_id": &account_id,
                        "from": &from,
                        "reply": &reply,
                    }),
                );
            }
            Err(e) => {
                println!("[WechatMessageService] LLM 调用失败: {}", e);
            }
        }
    }
}

/// 保存微信消息到数据库
async fn save_wechat_message(
    db_state: &DbState,
    account_id: &str,
    content: &str,
) -> Result<ChatMessageModel, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().naive_utc();

    let active_model = ActiveModel {
        id: Set(generate_id()),
        account_id: Set(account_id.to_string()),
        chat_type: Set("wechat".to_string()),
        session_id: Set("default".to_string()),
        parent_message_id: Set(None),
        role: Set("user".to_string()),
        content: Set(Some(content.to_string())),
        content_summary: Set(None),
        thinking: Set(None),
        tool_calls: Set(None),
        tool_call_id: Set(None),
        tool_output: Set(None),
        extends: Set("{}".to_string()),
        attachments: Set(None),
        status: Set("completed".to_string()),
        token_usage: Set(None),
        created_at: Set(now),
        metadata: Set("{}".to_string()),
        is_deleted: Set("0".to_string()),
    };

    active_model.insert(&*db).await.map_err(|e| e.to_string())
}

/// 发送系统通知
fn send_notification(app: &AppHandle, from: &str, body: &str) {
    let body_preview = if body.len() > 50 {
        format!("{}...", &body[..50])
    } else {
        body.to_string()
    };

    let state = app
        .notification()
        .permission_state()
        .unwrap_or(PermissionState::Denied);

    if state == PermissionState::Granted {
        let _ = app
            .notification()
            .builder()
            .title("收到新消息")
            .body(format!("来自 {}: {}", from, body_preview))
            .show();
    }
}

/// 发送回复到微信
async fn send_reply_to_wechat(
    client: &WechatClient,
    account_id: &str,
    to: &str,
    content: &str,
) -> Result<(), String> {
    let req = SendMessageRequest {
        account_id: account_id.to_string(),
        to: to.to_string(),
        text: content.to_string(),
    };

    client.send_message(req).await.map_err(|e| e.to_string())?;

    println!(
        "[WechatMessageService] 已发送微信回复 to={} content={}",
        to, content
    );

    Ok(())
}

/// 从数据库获取启用的 LLM 提供者配置
async fn get_enabled_provider(
    db_state: &DbState,
) -> Result<(ProviderConfigPayload, String), String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;

    // 查询启用的提供商配置
    let provider = mpc::Entity::find()
        .filter(mpc::Column::Enabled.eq(1))
        .order_by_asc(mpc::Column::SortIndex)
        .one(&*db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未找到启用的 LLM 提供商配置".to_string())?;

    // 构建 ProviderConfigPayload
    let config = match provider.provider_kind.as_str() {
        "open_ai" | "openai_compatible" => ProviderConfigPayload::OpenAiCompatible {
            base_url: provider.api_base_url,
            api_key: provider.api_key.unwrap_or_default(),
        },
        "anthropic" => ProviderConfigPayload::Anthropic {
            api_key: provider.api_key.unwrap_or_default(),
        },
        "ollama" => ProviderConfigPayload::Ollama {
            base_url: provider.api_base_url,
        },
        _ => {
            return Err(format!("不支持的提供商类型: {}", provider.provider_kind));
        }
    };

    Ok((config, provider.display_name))
}

/// 调用 LLM 获取回复（使用非流式 API）
async fn call_llm(
    db_state: &DbState,
    _account_id: &str,
    user_message: &str,
) -> Result<String, String> {
    let (provider_config, _provider_name) = get_enabled_provider(db_state).await?;

    // 构建聊天请求
    let req = ChatRequest {
        messages: vec![
            LlmChatMessage {
                role: Role::System,
                content: "你是一个智能助手，请简洁地回复用户的消息。".to_string(),
            },
            LlmChatMessage {
                role: Role::User,
                content: user_message.to_string(),
            },
        ],
        model: "".to_string(), // 非流式调用时会忽略这个
        temperature: 0.8,
        max_tokens: None,
    };

    // 使用 Provider 发送非流式请求
    let provider = Provider::try_from(provider_config).map_err(|e| e.to_string())?;
    let reply = provider
        .send_message(req)
        .await
        .map_err(|e| e.to_string())?;

    if reply.is_empty() {
        return Err("LLM 返回空回复".to_string());
    }

    Ok(reply)
}

/// 保存 LLM 回复到数据库
async fn save_llm_reply(
    db_state: &DbState,
    account_id: &str,
    content: &str,
) -> Result<ChatMessageModel, String> {
    let db = db_state.get().await.map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().naive_utc();

    let active_model = ActiveModel {
        id: Set(generate_id()),
        account_id: Set(account_id.to_string()),
        chat_type: Set("wechat".to_string()),
        session_id: Set("default".to_string()),
        parent_message_id: Set(None),
        role: Set("assistant".to_string()),
        content: Set(Some(content.to_string())),
        content_summary: Set(None),
        thinking: Set(None),
        tool_calls: Set(None),
        tool_call_id: Set(None),
        tool_output: Set(None),
        extends: Set("{}".to_string()),
        attachments: Set(None),
        status: Set("completed".to_string()),
        token_usage: Set(None),
        created_at: Set(now),
        metadata: Set("{}".to_string()),
        is_deleted: Set("0".to_string()),
    };

    active_model.insert(&*db).await.map_err(|e| e.to_string())
}
