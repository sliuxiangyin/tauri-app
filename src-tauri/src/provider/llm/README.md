# LLM 多厂商封装（`provider::llm`）

本目录在 Rust 侧统一「聊天补全 / 流式输出」的**领域层**：`LlmProvider`、各厂商适配与公共类型；**不包含** Tauri 命令（命令在 **`src-tauri/src/commands/llm.rs`**，通过 `crate::provider::llm` 调用此处能力）。

## 目录与职责

| 文件 | 说明 |
|------|------|
| `provider_trait.rs` | `LlmProvider`：`send_message`（非流式）、`stream_chat`（流式，返回统一事件流） |
| `types.rs` | `ChatMessage`、`ChatRequest`、`Role`、`ProviderConfigPayload`（前端传入的厂商 + 凭证） |
| `stream.rs` | `LlmStreamEvent`（厂商层片段）、`LlmChunkEnvelope`（带 `stream_id` 的 `llm:chunk` 载荷） |
| `error.rs` | `LlmError`：HTTP、JSON、空回复、配置错误等 |
| `openai_compatible.rs` | OpenAI Chat Completions 兼容实现（OpenAI、DeepSeek 等换 `base_url` 即可） |
| `anthropic.rs` | Anthropic Messages API |
| `ollama.rs` | Ollama `/api/chat` |
| `dispatcher.rs` | `Provider` 枚举、`TryFrom<ProviderConfigPayload>`、`impl LlmProvider for Provider` |

Tauri 命令定义在 **`src-tauri/src/commands/llm.rs`**（`commands::llm`），不在本目录内。

## Tauri 命令

命令注册在 `lib.rs` 的 `generate_handler` 中，路径为 **`commands::llm::...`**（宏生成的符号位于 `commands::llm` 模块）。

### `llm_chat_once(provider, req) -> Result<String, String>`

一次请求返回完整助手文本。参数与流式命令中的 `provider`、`req` 含义相同。

### `llm_chat_stream(stream_id, provider, req) -> Result<(), String>`

- **`stream_id`**（必填）：由前端生成的请求关联 ID（建议 `crypto.randomUUID()`），须非空；仅含空白会返回错误。所有 `llm:chunk` 与对应 `llm:error` 均携带同一 `stream_id`，便于在 `listen` 回调里过滤并发/连点。
- **provider**、**req**：含义见下文「ProviderConfigPayload」「ChatRequest」。

`AppHandle` 由 Tauri 注入，**不要**从前端传入。

推荐顺序：**先** `listen('llm:chunk' / 'llm:error')`，**再** `invoke('llm_chat_stream', { stream_id, provider, req })`，避免丢失首包。

### 前端事件

| 事件名 | 载荷 |
|--------|------|
| `llm:chunk` | 见下节 **`LlmChunkEnvelope`** |
| `llm:error` | `{ "stream_id": string, "message": string }`；随后仍会发一条 `llm:chunk`，`kind` 为 `done`（且同一 `stream_id`），便于 UI 收尾 |

`core:default` 已包含 `core:event:default`，一般无需为上述事件单独改 capability。

### `llm:chunk` 载荷形状（`LlmChunkEnvelope`）

JSON 为 **`stream_id` + 扁平后的 `LlmStreamEvent`**（`kind` 为 `text_delta` 或 `done`）：

```json
{ "stream_id": "550e8400-e29b-41d4-a716-446655440000", "kind": "text_delta", "text": "片段" }
{ "stream_id": "550e8400-e29b-41d4-a716-446655440000", "kind": "done" }
```

## `ProviderConfigPayload`（`invoke` 中的 `provider`）

使用 **`kind` 标签** 的 JSON。

**Tauri 前端 `invoke`**：由 CLI 生成的 IPC 约定里，OpenAI 兼容分支的标签为 **`open_ai`**（中间多一道下划线）；顶层参数名多为 **camelCase**（如 `streamId`）。Rust 侧已对 `kind` 与嵌套字段加了 `serde` 的 `rename` / `alias`，可同时接受 `open_ai` 与 `openai_compatible`，以及 `base_url` / `baseUrl` 等写法。

**OpenAI 兼容**（含 DeepSeek 等）：

```json
{
  "kind": "open_ai",
  "base_url": "https://api.openai.com",
  "api_key": "sk-..."
}
```

将 `base_url` 改为目标服务根地址即可（勿省略协议；末尾斜杠可有可无，内部会规范化拼接路径）。

**Anthropic**：

```json
{
  "kind": "anthropic",
  "api_key": "sk-ant-..."
}
```

**Ollama**：

```json
{
  "kind": "ollama",
  "base_url": "http://127.0.0.1:11434"
}
```

## `ChatRequest`（`invoke` 中的 `req`）

- **`messages`**：`{ "role": "system" \| "user" \| "assistant", "content": "..." }[]`
- **`model`**：厂商侧模型名
- **`temperature`**：可选，缺省为 `1.0`
- **`max_tokens`**：可选；Anthropic 在未指定时使用内部默认（见 `anthropic.rs`）

说明：Anthropic 会把 `role` 为 `system` 的消息从 `messages` 中抽出，合并为 API 的顶层 `system` 字段。

## 扩展新厂商

1. 新建实现文件，为结构体实现 `LlmProvider`（必要时复用 `openai_compatible` 或单独解析 SSE/NDJSON）。
2. 在 `types.rs` 的 `ProviderConfigPayload` 增加变体与字段。
3. 在 `dispatcher.rs` 的 `Provider` 与 `TryFrom`、`impl LlmProvider for Provider` 中增加分支。

## 安全提示

当前设计为**每次请求由前端传入密钥与 base_url**，便于开发联调。生产环境建议改为：密钥仅存本地安全存储、Rust 侧按配置 ID 解析，避免把 `api_key` 长期留在前端内存或日志中。
