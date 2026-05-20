// MCP 相关 API 调用
import { invoke } from '@tauri-apps/api/core';


export interface McpDto {
  id: number;
  name: string;
  transport: string;
  config: string;       // JSON 字符串
  status: string;       // "enable" | "disable"
  operating: string;
  tools?: string;
  error_msg?: string;
  updated_at: string;
}

// 获取所有 MCP 配置
export async function getAllMcps(): Promise<McpDto[]> {
  let data = await invoke<McpDto[]>('get_all_mcps');
  console.log("data: ", data);
  return data;
}

// 获取单个 MCP 配置
export async function getMcp(name: string): Promise<McpDto | null> {
  return invoke<McpDto | null>('get_mcp', { name });
}

// 创建 MCP 配置
export async function createMcp(
  name: string,
  config: string,  // JSON 字符串
  status: string = 'enable'
): Promise<McpDto> {
  return invoke<McpDto>('create_mcp', { name, config, status });
}

// 更新 MCP 配置
export async function updateMcp(
  name: string,
  config?: string,
  status?: string
): Promise<McpDto> {
  return invoke<McpDto>('update_mcp', { name, config, status });
}

// 删除 MCP 配置
export async function deleteMcp(name: string): Promise<void> {
  return invoke<void>('delete_mcp', { name });
}

// 切换 MCP 状态
export async function toggleMcpStatus(name: string): Promise<McpDto> {
  return invoke<McpDto>('toggle_mcp_status', { name });
}

