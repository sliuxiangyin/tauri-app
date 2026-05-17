/**
 * MCP 服务前端调用示例
 * 这个文件展示了如何在 React/Vue 等前端框架中使用 MCP 服务 API
 */

import { invoke } from '@tauri-apps/api/core';

// ============= 类型定义 =============

interface McpModelConfig {
  transport: 'stdio' | 'http';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
}

interface ConnectRequest {
  service_id: string;
  name?: string;
  config: McpModelConfig;
}

interface ToolInfo {
  name: string;
  description?: string;
  input_schema: Record<string, any>;
}

interface ToolCallResult {
  content: Array<{
    type: string;
    text?: string;
    data?: string;
    mime_type?: string;
  }>;
  is_error: boolean;
}
export type { ConnectRequest, ToolInfo, ToolCallResult };
// ============= API 封装 =============

export class McpService {
  /**
   * 连接 MCP 服务
   */
  static async connect(req: ConnectRequest): Promise<string> {
    try {
      const result = await invoke<string>('mcp_connect', { req });
      console.log(`✅ ${result}`);
      return result;
    } catch (error) {
      console.error('❌ 连接失败:', error);
      throw error;
    }
  }

  /**
   * 断开连接
   */
  static async disconnect(serviceId: string): Promise<string> {
    try {
      const result = await invoke<string>('mcp_disconnect', {
        service_id: serviceId,
      });
      console.log(`✅ ${result}`);
      return result;
    } catch (error) {
      console.error('❌ 断开连接失败:', error);
      throw error;
    }
  }

  /**
   * 获取工具列表
   */
  static async listTools(
    serviceId: string,
    forceRefresh: boolean = false
  ): Promise<ToolInfo[]> {
    try {
      const tools = await invoke<ToolInfo[]>('mcp_list_tools', {
        req: {
          service_id: serviceId,
          force_refresh: forceRefresh,
        },
      });
      console.log(
        `✅ 获取 ${tools.length} 个工具 (${forceRefresh ? '强制刷新' : '缓存'})`
      );
      return tools;
    } catch (error) {
      console.error('❌ 获取工具列表失败:', error);
      throw error;
    }
  }

  /**
   * 调用工具
   */
  static async callTool(
    serviceId: string,
    toolName: string,
    arguments_: Record<string, any>
  ): Promise<ToolCallResult> {
    try {
      const result = await invoke<ToolCallResult>('mcp_call_tool', {
        req: {
          service_id: serviceId,
          tool_name: toolName,
          arguments: arguments_,
        },
      });
      console.log(`✅ 工具 "${toolName}" 执行${result.is_error ? '失败' : '成功'}`);
      return result;
    } catch (error) {
      console.error(`❌ 工具 "${toolName}" 执行失败:`, error);
      throw error;
    }
  }

  /**
   * 列出所有已连接的服务
   */
  static async listServices(): Promise<any[]> {
    try {
      const services = await invoke<any[]>('mcp_list_services');
      console.log(`✅ 获取 ${services.length} 个已连接的服务`);
      return services;
    } catch (error) {
      console.error('❌ 获取服务列表失败:', error);
      throw error;
    }
  }

  /**
   * 检查服务连接状态
   */
  static async isConnected(serviceId: string): Promise<boolean> {
    try {
      const connected = await invoke<boolean>('mcp_is_service_connected', {
        service_id: serviceId,
      });
      console.log(`✅ 服务 "${serviceId}" 连接状态: ${connected ? '已连接' : '未连接'}`);
      return connected;
    } catch (error) {
      console.error('❌ 检查连接状态失败:', error);
      throw error;
    }
  }

  /**
   * 清除工具缓存
   */
  static async clearCache(serviceId: string): Promise<string> {
    try {
      const result = await invoke<string>('mcp_clear_tools_cache', {
        service_id: serviceId,
      });
      console.log(`✅ ${result}`);
      return result;
    } catch (error) {
      console.error('❌ 清除缓存失败:', error);
      throw error;
    }
  }
}

// ============= 使用示例 =============

/**
 * 示例 1: 连接本地 Stdio 服务
 */
export async function example1_localStdio() {
  console.log('\n========== 示例 1: 本地 Stdio 服务 ==========');

  try {
    // 1. 连接服务
    await McpService.connect({
      service_id: 'local-tools',
      name: 'Local Tools Server',
      config: {
        transport: 'stdio',
        command: '/opt/mcp-servers/tools-server',
        args: [],
        env: {},
      },
    });

    // 2. 获取工具列表
    const tools = await McpService.listTools('local-tools');
    console.log('可用工具:', tools.map((t) => t.name).join(', '));

    // 3. 调用工具
    if (tools.length > 0) {
      const firstTool = tools[0];
      const result = await McpService.callTool(
        'local-tools',
        firstTool.name,
        { example: 'param' }
      );
      console.log('工具结果:', result.content);
    }

    // 4. 断开连接
    await McpService.disconnect('local-tools');
  } catch (error) {
    console.error('示例 1 出错:', error);
  }
}

/**
 * 示例 2: 连接远程 HTTP 服务
 */
export async function example2_remoteHttp() {
  console.log('\n========== 示例 2: 远程 HTTP 服务 ==========');

  try {
    // 1. 连接服务
    await McpService.connect({
      service_id: 'remote-api',
      name: 'Remote API Server',
      config: {
        transport: 'http',
        url: 'http://api.example.com/mcp',
      },
    });

    // 2. 获取工具列表（使用缓存）
    const tools = await McpService.listTools('remote-api', false);
    console.log('缓存的工具:', tools.map((t) => t.name).join(', '));

    // 3. 强制刷新工具列表
    const freshTools = await McpService.listTools('remote-api', true);
    console.log('刷新的工具:', freshTools.map((t) => t.name).join(', '));

    // 4. 清除缓存
    await McpService.clearCache('remote-api');

    // 5. 断开连接
    await McpService.disconnect('remote-api');
  } catch (error) {
    console.error('示例 2 出错:', error);
  }
}

/**
 * 示例 3: 并行连接多个服务
 */
export async function example3_multipleServices() {
  console.log('\n========== 示例 3: 并行多服务 ==========');

  try {
    // 1. 并行连接多个服务
    await Promise.all([
      McpService.connect({
        service_id: 'service-1',
        name: 'Service 1',
        config: {
          transport: 'http',
          url: 'http://localhost:3001/mcp',
        },
      }),
      McpService.connect({
        service_id: 'service-2',
        name: 'Service 2',
        config: {
          transport: 'http',
          url: 'http://localhost:3002/mcp',
        },
      }),
      McpService.connect({
        service_id: 'service-3',
        name: 'Service 3',
        config: {
          transport: 'stdio',
          command: '/opt/mcp-servers/service-3',
        },
      }),
    ]);

    // 2. 列出所有服务
    const allServices = await McpService.listServices();
    console.log('所有服务:', allServices.map((s) => s.service_id).join(', '));

    // 3. 并行获取所有服务的工具
    const toolsResult = await Promise.all([
      McpService.listTools('service-1'),
      McpService.listTools('service-2'),
      McpService.listTools('service-3'),
    ]);

    toolsResult.forEach((tools, index) => {
      console.log(
        `Service ${index + 1} 工具:`,
        tools.map((t) => t.name).join(', ')
      );
    });

    // 4. 并行调用不同服务的工具
    const callResults = await Promise.all([
      McpService.callTool('service-1', 'tool1', { param: 'value1' }),
      McpService.callTool('service-2', 'tool2', { param: 'value2' }),
      McpService.callTool('service-3', 'tool3', { param: 'value3' }),
    ]);

    callResults.forEach((result, index) => {
      console.log(`Call ${index + 1}:`, result.is_error ? 'Error' : 'Success');
    });

    // 5. 并行断开所有连接
    await Promise.all([
      McpService.disconnect('service-1'),
      McpService.disconnect('service-2'),
      McpService.disconnect('service-3'),
    ]);
  } catch (error) {
    console.error('示例 3 出错:', error);
  }
}


/**
 * 示例 5: 错误处理
 */
export async function example5_errorHandling() {
  console.log('\n========== 示例 5: 错误处理 ==========');

  try {
    // 尝试连接不存在的服务
    await McpService.connect({
      service_id: 'nonexistent',
      name: 'Nonexistent Service',
      config: {
        transport: 'http',
        url: 'http://localhost:9999/mcp', // 不存在的端口
      },
    });
  } catch (error) {
    console.error('预期的错误:', error);
  }

  try {
    // 尝试调用不存在的服务的工具
    await McpService.listTools('nonexistent');
  } catch (error) {
    console.error('预期的错误:', error);
  }
}

// ============= 运行示例 =============

// 取消注释以下行来运行示例：
// example1_localStdio();
// example2_remoteHttp();
// example3_multipleServices();
// example5_errorHandling();

