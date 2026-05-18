// lib/chat-model-api.ts
// Chat Model 相关的 Tauri API 调用封装

import { invoke } from "@tauri-apps/api/core";

// ============== 类型定义 ==============

/** 当前账户的模型信息 DTO */
export interface AccountModelDto {
  config_id: string;
  display_name: string;
  model_id: string;
  model_name: string;
  is_default: boolean;
}

/** 模型项 */
export interface ModelItem {
  model_id: string;
  model_name: string;
}

/** 分组后的模型配置 */
export interface ModelGroup {
  name: string;
  id: string;
  items: ModelItem[];
}

// ============== API 调用函数 ==============

/**
 * 获取当前账户已选择的模型
 * - 如果账户已选择模型，返回该模型信息
 * - 如果未选择，返回第一个开启的模型
 * - 如果没有任何可用模型，返回错误
 */
export async function getChatModel(accountId: string): Promise<AccountModelDto> {
  const result = await invoke<AccountModelDto>("get_chat_model", { accountId });
  return result;
}

/**
 * 获取所有可用模型列表（按配置分组）
 */
export async function getAllChatModels(): Promise<ModelGroup[]> {
  const result = await invoke<ModelGroup[]>("get_all_chat_models");
  return result;
} 

/**
 * 设置账户的模型选择
 */
export async function setChatModel(
  accountId: string,
  configId: string,
  modelId: string
): Promise<AccountModelDto> {
  return invoke<AccountModelDto>("set_chat_model", {
    accountId,
    configId,
    modelId,
  });
}