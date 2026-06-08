// stores/useChatStore.ts
import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { AccountInfo } from '@/lib/api/wechat'
import { getMessages, getSessions, type MessageDto, type SessionDto } from '@/lib/api/messages'

// ──────────────────────────────────────────────────────────────
// 类型定义（直接使用后端 MessageDto / SessionDto）
// ──────────────────────────────────────────────────────────────

export type { MessageDto, SessionDto }

// 简化的存储实现：使用 localStorage（支持 session 持久化）
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

// ──────────────────────────────────────────────────────────────
// Store State
// ──────────────────────────────────────────────────────────────

interface ChatState {
  // 当前选中的账号信息
  selectedAccount: AccountInfo | null
  // 当前选中的会话（固定为 'default'）
  selectedSessionId: string
  // 当前会话的消息列表（使用后端 MessageDto 类型）
  messages: MessageDto[]
  // 会话列表
  sessions: SessionDto[]
  // 加载状态
  isLoading: boolean
  // 错误信息
  error: string | null

  // Actions
  setSelectedAccount: (account: AccountInfo) => void
  loadMessages: () => Promise<void>
  loadSessions: () => Promise<void>
  clearError: () => void
  initWebhookListener: () => Promise<UnlistenFn>
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
          const msgs = await getMessages(selectedAccount.accountId, 'default')
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
          const sessions = await getSessions(selectedAccount.accountId)
          set({ sessions })
        } catch (error) {
          console.error('加载会话列表失败:', error)
        }
      },

      // 清除错误
      clearError: () => {
        set({ error: null })
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
  )
)