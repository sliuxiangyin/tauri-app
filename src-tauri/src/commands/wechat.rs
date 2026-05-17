use std::sync::atomic::Ordering;

use futures_util::StreamExt;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::provider::wechat::{
    AccountsResponse, SendMessageRequest, SendMessageResponse, WechatClient,
};

#[tauri::command]
pub async fn wechat_login_stream(
    app: AppHandle,
    account_id: String,
    client: State<'_, WechatClient>,
) -> Result<(), String> {
    let (mut stream, abort_flag) = client
        .login_stream(&account_id)
        .await
        .map_err(|e| e.to_string())?;

    loop {
        tokio::select! {
            // 分支1：SSE 流事件到达 → 正常处理并 emit 到前端
            item = stream.next() => {
                match item {
                    Some(Ok(event)) => {
                        let _ = app.emit("wechat:login_event", &json!(event));
                    }
                    Some(Err(e)) => {
                        let _ = app.emit(
                            "wechat:login_error",
                            json!({
                                "accountId": &account_id,
                                "message": e.to_string(),
                            }),
                        );
                        return Ok(());
                    }
                    None => break, // 流正常结束
                }
            }
            // 分支2：每 200ms 主动轮询取消标记，即使 SSE 服务端无数据也能响应取消
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(200)) => {
                if abort_flag.load(Ordering::SeqCst) {
                    println!("[Command] wechat_login_stream abort detected via select!");
                    return Ok(());
                }
            }
        }
    }
    println!("[Command] wechat_login_stream stream ended naturally");
    Ok(())
}

/// 取消当前的登录流，断开与微信服务端的连接
#[tauri::command]
pub async fn wechat_login_cancel(client: State<'_, WechatClient>) -> Result<(), String> {
    println!("[Command] wechat_login_cancel called");
    client.cancel_login_stream().await;
    Ok(())
}

#[tauri::command]
pub async fn wechat_send_message(
    req: SendMessageRequest,
    client: State<'_, WechatClient>,
) -> Result<SendMessageResponse, String> {
    client.send_message(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn wechat_get_accounts(
    client: State<'_, WechatClient>,
) -> Result<AccountsResponse, String> {
    client.get_accounts().await.map_err(|e| e.to_string())
}
