import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ChatRole = "system" | "user" | "assistant";

export type ChatMessagePayload = {
  role: ChatRole;
  content: string;
};



/** 与 `invoke('llm_chat_stream')` 的 `provider` 一致；`kind` 须为 Tauri 生成的 `open_ai` 。 */
export type ProviderConfigPayload =
  | { kind: "open_ai"; base_url: string; api_key: string }
  | { kind: "anthropic"; api_key: string }
  | { kind: "ollama"; base_url: string };

export type LlmChunkPayload =
  | { account_id: string; kind: "text_delta"; text: string }
  | { account_id: string; kind: "done" }
  // 思考链
  | { account_id: string; kind: "reasoning_delta"; text: string }
  // 工具调用
  | { account_id: string; kind: "tool_call_start"; index: number; id: string; name: string }
  | { account_id: string; kind: "tool_call_delta"; index: number; arguments: string }
  | { account_id: string; kind: "tool_call_done"; index: number; arguments: unknown }
  | { account_id: string; kind: "tool_result"; call_id: string; name: string; result: unknown; success: boolean }
  // Block 边界
  | { account_id: string; kind: "block_start"; block_type: string; order_num: number }
  // Agent Plan 步骤
  | { account_id: string; kind: "plan_steps"; reasoning: string; steps: PlanStepDto[] }
  // 引用
  | { account_id: string; kind: "reference"; source_type: string; title: string; url: string; snippet?: string }
  // 音频
  | { account_id: string; kind: "audio_delta"; data: string; format: string }
  // 错误与警告
  | { account_id: string; kind: "error"; code: string; message: string }
  | { account_id: string; kind: "warning"; code: string; message: string }
  // 元数据
  | { account_id: string; kind: "usage"; input_tokens: number; output_tokens: number; reasoning_tokens?: number }
  | { account_id: string; kind: "metadata"; model: string; finish_reason?: string; request_id?: string };

/** Plan 步骤（与 Rust PlanStep 对齐） */
export interface PlanStepDto {
  order: number;
  step_type: string;
  tool_name: string;
  parameters: unknown;
  step_goal: string;
  expected_output?: string;
  depends_on?: number;
}

export type LlmErrorPayload = {
  account_id: string;
  message: string;
};

/** 与 `src-tauri/src/provider/llm/types.rs` 的 `ProviderConfigPayload` 对齐；在此对象里写死本地配置即可。 */
// export const LLM_LOCAL = {
//   provider: {
//     kind: "open_ai",
//     base_url: "https://api.deepseek.com",
//     api_key: "sk-bd8f680b2675464484ad1fa8b511fe6f",
//   } satisfies ProviderConfigPayload,
//   model: "deepseek-chat",
//   temperature: 1,
// } as const;

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * 取消 LLM 流式聊天
 */
export async function cancelLlmChat(accountId: string): Promise<boolean> {
  return invoke<boolean>("llm_chat_cancel", { accountId });
}

/**
 * 先注册 `listen`，再 `invoke`，与后端 README 约定一致。
 * `invoke` 在流结束后才 resolve；期间通过回调推送 chunk。
 *
 * 后端会自动从数据库查询该账户的历史消息并组装上下文，
 * 前端只需传递当前用户输入的 content。
 *
 * @returns 包含 cancel 方法的对象，可用于中断流式响应
 */
export async function streamLlmChat(options: {
  accountId: string;
  sessionId?: string;
  messages: ChatMessagePayload[];
  onChunk: (payload: LlmChunkPayload) => void;
  onStreamError: (payload: LlmErrorPayload) => void;
  onStreamEnd?: () => void | Promise<void>;
}): Promise<{ cancel: () => Promise<boolean> }> {
  const { accountId, sessionId = "default", messages, onChunk, onStreamError, onStreamEnd } = options;

  // 保存 unlisten 函数，用于 cancel 时清理
  let unlistenChunk: (() => void) | null = null;
  let unlistenErr: (() => void) | null = null;

  const unlistenChunkPromise = listen<LlmChunkPayload>("llm:chunk", (event) => {
    if (event.payload.account_id !== accountId) {
      return;
    }
    onChunk(event.payload);
  });

  const unlistenErrPromise = listen<LlmErrorPayload>("llm:error", (event) => {
    if (event.payload.account_id !== accountId) {
      return;
    }
    onStreamError(event.payload);
  });

  // 等待 listen 完成后再执行 invoke
  const [chunkListener, errListener] = await Promise.all([
    unlistenChunkPromise,
    unlistenErrPromise,
  ]);
  unlistenChunk = chunkListener;
  unlistenErr = errListener;

  try {
    await invoke("llm_chat_stream", {
      accountId,
      sessionId,
      messages,
    });
    // 流结束，调用回调
    await onStreamEnd?.();
  } finally {
    unlistenChunk?.();
    unlistenErr?.();
  }

  // 返回 cancel 方法，调用 llm_chat_cancel 并清理监听器
  return {
    cancel: async () => {
      // 先清理监听器
      unlistenChunk?.();
      unlistenErr?.();
      unlistenChunk = null;
      unlistenErr = null;
      // 调用后端取消
      return cancelLlmChat(accountId);
    },
  };
}
