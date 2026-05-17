// stores/useChatStore.ts
import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { AccountInfo } from '@/lib/wechat-api'

// 简化的存储实现：使用 localStorage（支持 session 持久化）
// 如果需要更安全的存储（如加密），未来可以切换到 @tauri-apps/plugin-store
const browserStorage = {
  getItem: async (name: string): Promise<string | null> => {
    return window.localStorage.getItem(name)
  },
  setItem: async (name: string, value: string): Promise<void> => {
    window.localStorage.setItem(name, value)
  },
  removeItem: async (name: string): Promise<void> => {
    window.localStorage.removeItem(name)
  },
}

export interface ChatMessage {
  id: string
  account_id: string
  chat_type: 'client' | 'wechat'
  session_id: string
  parent_message_id?: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  content_summary?: string
  thinking?: string
  tool_calls?: string
  tool_call_id?: string
  tool_output?: string
  status: 'pending' | 'completed' | 'failed'
  created_at: string
}

export interface ChatSession {
  session_id: string
  name: string
  message_count: number
  last_message_at?: string
}

interface ChatState {
  // 当前选中的账号信息
  selectedAccount: AccountInfo | null
  // 当前选中的会话（固定为 'default'）
  selectedSessionId: string
  // 当前会话的消息列表
  messages: ChatMessage[]
  // 会话列表
  sessions: ChatSession[]
  // 加载状态
  isLoading: boolean
  // 错误信息
  error: string | null

  // Actions
  setSelectedAccount: (account: AccountInfo) => void
  loadMessages: () => Promise<void>
  loadSessions: () => Promise<void>
  addMessage: (message: ChatMessage) => void
  updateMessage: (id: string, updates: Partial<ChatMessage>) => void
  clearError: () => void
  initWebhookListener: () => Promise<UnlistenFn>
  persistSelectedAccount: (account: AccountInfo) => Promise<void>
}

// 存储取消监听函数
let unlistenWechatMessage: UnlistenFn | null = null
let unlistenLlmReply: UnlistenFn | null = null

export const useChatStore = create<ChatState>()(
  persist(
    (set, get) => ({
  // 默认状态
  selectedAccount: null,
  selectedSessionId: 'default',
  messages: [],
  sessions: [],
  isLoading: false,
  error: null,

  // 设置选中的账号并加载数据
  setSelectedAccount: (account: AccountInfo) => {
    set({ selectedAccount: account, selectedSessionId: 'default' })
    // 加载该账号的数据
    get().loadMessages()
    get().loadSessions()
  },

  // 加载消息列表
  loadMessages: async () => {
    const { selectedAccount } = get()
    if (!selectedAccount) {
      set({ messages: [] })
      return
    }

    set({ isLoading: true, error: null })
    // 短暂延迟确保 UI 能感知到加载状态
    await new Promise(r => setTimeout(r, 300))
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const msgs = await invoke<ChatMessage[]>('get_messages', {
        accountId: selectedAccount.accountId,
        sessionId: 'default',
      })
      set({ messages: msgs, isLoading: false })
    } catch (error) {
      console.error('加载消息失败:', error)
      set({ error: String(error), isLoading: false })
    }
  },

  // 加载会话列表
  loadSessions: async () => {
    const { selectedAccount } = get()
    if (!selectedAccount) {
      set({ sessions: [] })
      return
    }

    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const sessions = await invoke<ChatSession[]>('get_sessions', {
        accountId: selectedAccount.accountId,
      })
      set({ sessions })
    } catch (error) {
      console.error('加载会话列表失败:', error)
    }
  },

  // 添加消息到列表
  addMessage: (message: ChatMessage) => {
    set((state) => ({
      messages: [...state.messages, message],
    }))
  },

  // 更新消息
  updateMessage: (id: string, updates: Partial<ChatMessage>) => {
    set((state) => ({
      messages: state.messages.map((msg) =>
        msg.id === id ? { ...msg, ...updates } : msg
      ),
    }))
  },

  // 清除错误
  clearError: () => {
    set({ error: null })
  },

  // 持久化保存选中的账号（通过 zustand persist middleware）
  persistSelectedAccount: async (account: AccountInfo) => {
    // 注意：账号现在通过 zustand persist middleware 自动持久化
    // 这个方法保留但不再需要手动调用
    console.log('[persistSelectedAccount] 账号已自动持久化:', account.accountId)
  },

  // 初始化 Webhook 消息监听
  initWebhookListener: async () => {
    // 清理旧的监听器
    if (unlistenWechatMessage) {
      unlistenWechatMessage()
    }
    if (unlistenLlmReply) {
      unlistenLlmReply()
    }

    // 监听微信消息（from Webhook）
    unlistenWechatMessage = await listen<{
      account_id: string
      from: string
      body: string
      to: string
    }>('wechat:message', (event) => {
      const { account_id } = event.payload
      const { selectedAccount } = get()

      // 只处理当前选中的账号
      if (selectedAccount?.accountId === account_id) {
        // 刷新消息列表以获取刚落库的消息
        get().loadMessages()
      }
    })

    // 监听 LLM 回复
    unlistenLlmReply = await listen<{
      account_id: string
      from: string
      reply: string
    }>('wechat:llm_reply', (event) => {
      const { account_id } = event.payload
      const { selectedAccount } = get()

      // 只处理当前选中的账号
      if (selectedAccount?.accountId === account_id) {
        // 刷新消息列表以获取 LLM 回复
        get().loadMessages()
      }
    })

    // 返回清理函数
    return () => {
      unlistenWechatMessage?.()
      unlistenLlmReply?.()
      unlistenWechatMessage = null
      unlistenLlmReply = null
    }
  },
}),
  {
    name: 'chat-store',
    storage: createJSONStorage(() => browserStorage),
    // 只持久化 selectedAccount，不持久化其他状态
    partialize: (state) => ({ selectedAccount: state.selectedAccount }),
  }
))