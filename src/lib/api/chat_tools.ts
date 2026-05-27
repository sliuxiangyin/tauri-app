import { invoke } from "@tauri-apps/api/core";

/**
 * 聊天工具权限配置
 */
export interface ChatToolsConfig {
  /** 被禁用的 MCP Server 列表 */
  disabled_servers: string[];
  /** 每个启用的 Server 中被禁用的工具列表 */
  disabled_tools: Record<string, string[]>;
}

/**
 * 获取指定 accountId + sessionId 的工具权限配置
 */
export async function getChatToolsConfig(
  accountId: string,
  sessionId: string
): Promise<ChatToolsConfig> {
  return invoke<ChatToolsConfig>("get_chat_tools_config", {
    accountId,
    sessionId,
  });
}

/**
 * 保存指定 accountId + sessionId 的工具权限配置
 */
export async function saveChatToolsConfig(
  accountId: string,
  sessionId: string,
  config: ChatToolsConfig
): Promise<void> {
  return invoke<void>("save_chat_tools_config", {
    accountId,
    sessionId,
    config,
  });
}

/**
 * 删除指定 accountId + sessionId 的工具权限配置（恢复默认）
 */
export async function deleteChatToolsConfig(
  accountId: string,
  sessionId: string
): Promise<void> {
  return invoke<void>("delete_chat_tools_config", {
    accountId,
    sessionId,
  });
}
