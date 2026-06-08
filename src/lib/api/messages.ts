import { invoke } from '@tauri-apps/api/core'

// ──────────────────────────────────────────────────────────────
// 类型定义（与 backend MessageDto / SessionDto 对齐）
// ──────────────────────────────────────────────────────────────

/** 内容块 DTO */
export interface ContentBlockDto {
  id: string
  mid: string
  block_type: 'text' | 'thinking' | 'tool'
  order_num: number
  source: string
  source_id?: string
  step_index?: number
  content?: string
  content_summary?: string
  thinking?: string
  tool_name?: string
  tool_arguments?: string
  tool_output?: string
  tool_status?: string
  tool_duration_ms?: number
  tool_error?: string
  extends: string
  attachments?: string
  metadata: string
  created_at: string
}

/** Plan DTO */
export interface PlanDto {
  id: string
  mid: string
  need_agent: string
  order_num: number
  reasoning?: string
  steps?: string
  step_results?: string
  stop_reason?: string
  completed_at?: string
  created_at: string
}

/** 统一内容项（blocks + plan 合并，按 order_num 排序） */
export type ContentItem =
  | { type: 'block'; data: ContentBlockDto }
  | { type: 'plan'; data: PlanDto }

/** 消息 DTO */
export interface MessageDto {
  id: string
  account_id: string
  chat_type: string
  session_id: string
  parent_id?: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  status: 'pending' | 'completed' | 'failed'
  token_usage?: string
  created_at: string
  is_deleted: '0' | '1'
  /** 按 order_num 排序的统一内容序列（blocks + plan 合并） */
  content: ContentItem[]
}

/** 会话 DTO */
export interface SessionDto {
  session_id: string
  name: string
  message_count: number
  last_message_at?: string
}

// ──────────────────────────────────────────────────────────────
// API 函数
// ──────────────────────────────────────────────────────────────

/**
 * 获取消息列表（含内容块和 Plan）
 * @param accountId 账号 ID
 * @param sessionId 会话 ID（可选，默认 "default"）
 * @param chatType 聊天类型（可选）
 * @param limit 返回条数限制（可选，默认 50）
 * @param offset 偏移量（可选，默认 0）
 */
export async function getMessages(
  accountId: string,
  sessionId?: string,
  chatType?: string,
  limit?: number,
  offset?: number
): Promise<MessageDto[]> {
  return invoke('get_messages', {
    accountId,
    sessionId: sessionId ?? null,
    chatType: chatType ?? null,
    limit: limit ?? null,
    offset: offset ?? null,
  })
}

/**
 * 清空消息（硬删除 + 级联删除 conversations 和 plans）
 * @param accountId 账号 ID
 * @param sessionId 会话 ID（可选）
 * @param chatType 聊天类型（可选）
 * @returns 删除的消息数量
 */
export async function clearMessages(
  accountId: string,
  sessionId?: string,
  chatType?: string
): Promise<number> {
  return invoke('clear_messages', {
    accountId,
    sessionId: sessionId ?? null,
    chatType: chatType ?? null,
  })
}

/**
 * 获取会话列表
 * @param accountId 账号 ID
 */
export async function getSessions(accountId: string): Promise<SessionDto[]> {
  return invoke('get_sessions', { accountId })
}