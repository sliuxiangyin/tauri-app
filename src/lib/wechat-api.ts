// /home/code/wclaw-v2/tauri-app/src/lib/wechat-api.ts

import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";


// ============ 类型定义 ============

export interface QrGeneratedData {
  qrDataUrl: string;
  sessionKey: string;
  message: string;
}

export interface ScannedData {
  message: string;
}

export interface QrExpiredData {
  retry_count: number;
  max_retries: number;
  message: string;
}

export interface ConfirmedData {
  account_id: string;
  message: string;
}

export interface LoginSuccessData {
  account_id: string;
  message: string;
}

export interface LoginFailedData {
  message: string;
}

export interface ErrorData {
  message: string;
}

export type WechatLoginEvent =
  | { event_type: 'qr_generated'; data: QrGeneratedData }
  | { event_type: 'scanned'; data: ScannedData }
  | { event_type: 'qr_expired'; data: QrExpiredData }
  | { event_type: 'confirmed'; data: ConfirmedData }
  | { event_type: 'login_success'; data: LoginSuccessData }
  | { event_type: 'login_failed'; data: LoginFailedData }
  | { event_type: 'error'; data: ErrorData };

export interface SendMessageRequest {
  account_id: string;
  to: string;
  text: string;
}

export interface SendMessageResponse {
  success: boolean;
  messageId: string;
  channel: string;
}

export interface AccountInfo {
  accountId: string;
  configured: boolean;
  enabled: boolean;
  running: boolean;
}

export interface AccountsResponse {
  success: boolean;
  accountIds: string[];
  accounts: AccountInfo[];
  count: number;
  runningCount: number;
}

export interface LoginError {
  accountId: string;
  message: string;
}

// ============ 类型定义：Webhook 消息 ============

export interface WebhookMessage {
  body: string;
  from: string;
  to: string;
  accountId: string;
  originatingChannel: string;
  originatingTo: string;
  messageSid: string;
  timestamp: number;
  provider: string;
  chatType: string;
  contextToken: string;
  mediaPath?: string;
  mediaType?: string;
  commandBody: string;
  commandAuthorized: boolean;
}

// ============ API 封装 ============

/**
 * 启动微信登录流
 * @param accountId - 账号 ID（可选，不传则使用临时会话）
 * @param onEvent - 登录事件回调
 * @param onError - 错误回调
 * @returns 取消监听函数
 */
export async function startLoginStream(
  accountId?: string,
  onEvent?: (event: WechatLoginEvent) => void,
  onError?: (error: LoginError) => void
): Promise<UnlistenFn> {
  // 保存回调引用，用于事件处理
  let eventCallback = onEvent;
  let errorCallback = onError;

  // 监听登录事件
  const unlistenEvent = await listen<WechatLoginEvent>('wechat:login_event', (event) => {
    if (eventCallback) {
      eventCallback(event.payload);
    }
  });

  // 监听登录错误
  const unlistenError = await listen<LoginError>('wechat:login_error', (event) => {
    if (errorCallback) {
      errorCallback(event.payload);
    }
  });

  // 调用后端命令启动登录流
  await invoke('wechat_login_stream', { accountId: accountId });

  // 返回取消监听函数
  return async () => {
    console.log('[startLoginStream] cleanup called, invoking cancel');
    // 先调用取消命令断开微信服务端连接
    try {
      await invoke('wechat_login_cancel');
    } catch (e) {
      console.warn('wechat_login_cancel failed:', e);
    }
    // 再取消前端事件监听
    console.log('[startLoginStream] unlistening events');
    await unlistenEvent();
    console.log('[startLoginStream] unlisten completed');
    await unlistenError();
  };
}

/**
 * 启动 Webhook 消息监听
 * @param onMessage - 消息回调
 * @returns 取消监听函数
 *
 * 注意：Rust 后端监听任务已在应用启动时自动启动（lib.rs setup 中），
 * 前端只需调用此函数建立事件监听即可接收消息。
 *
 * 中断场景与恢复机制：
 * 1. 前端页面刷新/重载：Rust 后台任务继续运行，但前端事件监听器丢失。
 *    页面重新加载后需要重新调用此函数建立监听。
 * 2. 前端窗口最小化：不影响监听，Rust 后台任务持续运行。
 * 3. 应用完全关闭：Rust 进程终止，重新打开应用后自动恢复。
 * 4. broadcast 通道 lag：如果前端处理消息过慢，可能错过部分消息（broadcast
 *    缓冲区满时旧消息被丢弃）。建议前端尽快处理回调，避免阻塞。
 */
export async function startMessageListener(
  onMessage: (message: WebhookMessage) => void
): Promise<UnlistenFn> {
  const unlisten = await listen<WebhookMessage>('wechat:message', (event) => {
    onMessage(event.payload);
  });

  return unlisten;
}

/**
 * 发送消息
 * @param req - 消息请求
 * @returns 消息发送响应
 */
export async function sendMessage(req: SendMessageRequest): Promise<SendMessageResponse> {
  return invoke<SendMessageResponse>('wechat_send_message', { req });
}

/**
 * 获取账号列表
 * @returns 账号列表响应
 */
export async function getAccounts(): Promise<AccountsResponse> {
  let response = await invoke<AccountsResponse>('wechat_get_accounts');
  console.log(response);
  return response;
}