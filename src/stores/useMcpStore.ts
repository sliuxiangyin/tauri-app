// stores/useMcpStore.ts
// MCP 状态管理 Store - 监听 MCP 服务器状态变化事件
import { create } from 'zustand'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'

// ============= 类型定义 =============

export interface ToolWithSource {
  server_id: string
  server_name: string
  name: string
  description?: string
  input_schema: Record<string, unknown>
}

export interface McpModelConfig {
  transport: 'stdio' | 'http'
  command?: string
  args?: string[]
  env?: Record<string, string>
  url?: string
}

export interface McpServeConfig {
  id: number
  name: string
  config: McpModelConfig
  state: boolean
  tools: ToolWithSource[]
  error: string | null
  updated_at: string
  install_status?: 'installing' | 'connected' | 'failed'
}

export interface CreateMcpServeConfigPayload {
  name: string
  config: McpModelConfig
}

export interface UpdateMcpServeConfigPayload {
  name?: string
  config?: McpModelConfig
}

// MCP 服务器状态变化事件 Payload
export interface McpServerEventPayload {
  server_id: string
  name: string
  status: 'installing' | 'connected' | 'failed' | 'removed'
  tool_count: number
  error?: string
}

// ============= Store 接口 =============

interface McpState {
  // 状态
  configs: McpServeConfig[]
  tools: ToolWithSource[]
  loading: boolean
  error: string | null

  // Actions
  loadConfigs: () => Promise<void>
  createConfig: (payload: CreateMcpServeConfigPayload) => Promise<McpServeConfig>
  updateConfig: (id: number, payload: UpdateMcpServeConfigPayload) => Promise<McpServeConfig>
  deleteConfig: (id: number) => Promise<void>
  refreshTools: () => Promise<void>
  getAvailableTools: () => ToolWithSource[]
  initEventListener: () => Promise<UnlistenFn>
  clearError: () => void
}

// ============= 存储实现 =============

let unlistenMcpServerChanged: UnlistenFn | null = null

export const useMcpStore = create<McpState>()((set, get) => ({
  // 默认状态
  configs: [],
  tools: [],
  loading: false,
  error: null,

  // 加载配置列表
  loadConfigs: async () => {
    set({ loading: true, error: null })
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const configs = await invoke<McpServeConfig[]>('list_mcp_serve_configs')
      set({ configs, loading: false })

      // 更新全局工具列表
      const allTools = configs.flatMap((c) => c.tools)
      set({ tools: allTools })
    } catch (error) {
      console.error('加载 MCP 配置失败:', error)
      set({ error: String(error), loading: false })
    }
  },

  // 创建配置（异步安装）
  createConfig: async (payload: CreateMcpServeConfigPayload) => {
    set({ loading: true, error: null })
    try {
      const { invoke } = await import('@tauri-apps/api/core')

      // 添加到本地状态，标记为安装中
      const tempConfig: McpServeConfig = {
        id: Date.now(), // 临时 ID，后续会更新
        name: payload.name,
        config: payload.config,
        state: false,
        tools: [],
        error: null,
        updated_at: new Date().toISOString(),
        install_status: 'installing',
      }

      set((state) => ({
        configs: [...state.configs, tempConfig],
      }))

      // 调用后端创建
      const config = await invoke<McpServeConfig>('create_mcp_serve_config', { payload })

      // 更新临时配置为真实配置
      set((state) => ({
        configs: state.configs.map((c) =>
          c.id === tempConfig.id ? { ...config, install_status: 'installing' as const } : c
        ),
        loading: false,
      }))

      return config
    } catch (error) {
      console.error('创建 MCP 配置失败:', error)
      set({ error: String(error), loading: false })
      throw error
    }
  },

  // 更新配置
  updateConfig: async (id: number, payload: UpdateMcpServeConfigPayload) => {
    set({ loading: true, error: null })
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const config = await invoke<McpServeConfig>('update_mcp_serve_config', { id, payload })

      // 标记为安装中
      set((state) => ({
        configs: state.configs.map((c) => (c.id === id ? { ...config, install_status: 'installing' as const } : c)),
        loading: false,
      }))

      return config
    } catch (error) {
      console.error('更新 MCP 配置失败:', error)
      set({ error: String(error), loading: false })
      throw error
    }
  },

  // 删除配置
  deleteConfig: async (id: number) => {
    set({ loading: true, error: null })
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke<void>('delete_mcp_serve_config', { id })

      set((state) => ({
        configs: state.configs.filter((c) => c.id !== id),
        loading: false,
      }))
    } catch (error) {
      console.error('删除 MCP 配置失败:', error)
      set({ error: String(error), loading: false })
      throw error
    }
  },

  // 刷新工具列表
  refreshTools: async () => {
    await get().loadConfigs()
  },

  // 获取可用工具列表
  getAvailableTools: () => {
    return get().tools
  },

  // 初始化事件监听
  initEventListener: async () => {
    // 清理旧的监听器
    if (unlistenMcpServerChanged) {
      unlistenMcpServerChanged()
    }

    unlistenMcpServerChanged = await listen<McpServerEventPayload>(
      'mcp:server-changed',
      (event) => {
        const { server_id, status, tool_count, error } = event.payload
        console.log('[MCP Store] 收到事件:', event.payload)

        set((state) => {
          const configIndex = state.configs.findIndex((c) => c.id.toString() === server_id)

          if (status === 'removed') {
            // 服务器被移除
            return {
              configs: state.configs.filter((c) => c.id.toString() !== server_id),
              tools: state.tools.filter((t) => t.server_id !== server_id),
            }
          }

          if (configIndex === -1) {
            // 未找到配置，忽略
            return state
          }

          const config = state.configs[configIndex]

          switch (status) {
            case 'installing':
              // 正在安装
              return {
                configs: state.configs.map((c, i) =>
                  i === configIndex ? { ...c, install_status: 'installing' as const, error: null } : c
                ),
              }

            case 'connected':
              // 连接成功，重新加载配置以获取工具列表
              get().loadConfigs()
              return {
                configs: state.configs.map((c, i) =>
                  i === configIndex
                    ? { ...c, state: true, install_status: 'connected' as const, error: null, tool_count }
                    : c
                ),
              }

            case 'failed':
              // 安装失败
              return {
                configs: state.configs.map((c, i) =>
                  i === configIndex
                    ? { ...c, state: false, install_status: 'failed' as const, error: error || '安装失败' }
                    : c
                ),
              }

            default:
              return state
          }
        })
      }
    )

    return () => {
      unlistenMcpServerChanged?.()
      unlistenMcpServerChanged = null
    }
  },

  // 清除错误
  clearError: () => {
    set({ error: null })
  },
}))