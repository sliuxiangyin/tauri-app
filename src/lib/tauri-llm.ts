import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ChatRole = "system" | "user" | "assistant";

export type ChatMessagePayload = {
  role: ChatRole;
  content: string;
};

export type ChatRequestPayload = {
  messages: ChatMessagePayload[];
  model: string;
  temperature?: number;
  max_tokens?: number | null;
};

/** 与 `invoke('llm_chat_stream')` 的 `provider` 一致；`kind` 须为 Tauri 生成的 `open_ai` 。 */
export type ProviderConfigPayload =
  | { kind: "open_ai"; base_url: string; api_key: string }
  | { kind: "anthropic"; api_key: string }
  | { kind: "ollama"; base_url: string };

export type LlmChunkPayload =
  | { stream_id: string; kind: "text_delta"; text: string }
  | { stream_id: string; kind: "done" };

export type LlmErrorPayload = {
  stream_id: string;
  message: string;
};

/** 与 `src-tauri/src/provider/llm/types.rs` 的 `ProviderConfigPayload` 对齐；在此对象里写死本地配置即可。 */
export const LLM_LOCAL = {
  provider: {
    kind: "open_ai",
    base_url: "https://api.deepseek.com",
    api_key: "sk-bd8f680b2675464484ad1fa8b511fe6f",
  } satisfies ProviderConfigPayload,
  model: "deepseek-chat",
  temperature: 1,
} as const;

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * 先注册 `listen`，再 `invoke`，与后端 README 约定一致。
 * `invoke` 在流结束后才 resolve；期间通过回调推送 chunk。
 */
export async function streamLlmChat(options: {
  streamId: string;
  provider: ProviderConfigPayload;
  req: ChatRequestPayload;
  onChunk: (payload: LlmChunkPayload) => void;
  onStreamError: (payload: LlmErrorPayload) => void;
}): Promise<void> {
  const { streamId, provider, req, onChunk, onStreamError } = options;

  const unlistenChunk = await listen<LlmChunkPayload>("llm:chunk", (event) => {
    console.log("[llm:chunk]", event);
    if (event.payload.stream_id !== streamId) {
      return;
    }
    onChunk(event.payload);
  });

  const unlistenErr = await listen<LlmErrorPayload>("llm:error", (event) => {
    if (event.payload.stream_id !== streamId) {
      return;
    }
    onStreamError(event.payload);
  });

  try {
    await invoke("llm_chat_stream", {
      streamId,
      provider,
      req,
    });
  } finally {
    unlistenChunk();
    unlistenErr();
  }
}
