"use client";

import { useRef, useState, useEffect } from "react";
import { nanoid } from "nanoid";
import {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import {
  Message,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
import { ChatBox } from "@/components/chat-page/chat-box";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  isTauriRuntime,
  streamLlmChat,
  type LlmChunkPayload,
} from "@/lib/api/tauri-llm";
import {
  IconAdjustmentsHorizontal,
  IconTrash,
} from "@tabler/icons-react";
import { type ChatMessage } from "@/stores/useChatStore";
import { type AccountInfo } from "@/lib/api/wechat";
import { clearMessages } from "@/lib/api/chat";
import {
  getChatModel,
  getAllChatModels,
  type AccountModelDto,
  type ModelGroup,
} from "@/lib/api/chat-model";
import {
  Dialog,
  DialogContent,
} from "@/components/ui/dialog";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";

// 渲染 LLM chunk 事件的辅助函数
function renderChunkEvent(payload: LlmChunkPayload): React.ReactNode {
  switch (payload.kind) {
    case "reasoning_delta":
      return <span className="text-blue-400">[思考] {payload.text}</span>;

    case "tool_call_start":
      return <span className="text-orange-400">[工具开始] {payload.name}</span>;

    case "tool_call_delta":
      return <span className="text-yellow-400">[工具参数] {payload.arguments}</span>;

    case "tool_call_done":
      return (
        <span className="text-orange-400">
          [工具完成] 参数: {JSON.stringify(payload.arguments)}
        </span>
      );

    case "tool_result":
      return (
        <span className={payload.success ? "text-green-400" : "text-red-400"}>
          [工具结果-{payload.success ? "成功" : "失败"}] {payload.name}: {JSON.stringify(payload.result)}
        </span>
      );

    case "reference":
      return (
        <span className="text-purple-400">
          [引用] {payload.title} - {payload.url}
        </span>
      );

    case "audio_delta":
      return <span className="text-cyan-400">[音频] {payload.format}</span>;

    case "usage":
      return (
        <span className="text-gray-400">
          [使用量] 输入: {payload.input_tokens}, 输出: {payload.output_tokens}
          {payload.reasoning_tokens !== undefined && `, 思考: ${payload.reasoning_tokens}`}
        </span>
      );

    case "metadata":
      return (
        <span className="text-gray-400">
          [元数据] 模型: {payload.model}, 结束原因: {payload.finish_reason || "无"}
        </span>
      );

    case "error":
      return <span className="text-red-400">[错误] {payload.code}: {payload.message}</span>;

    case "warning":
      return <span className="text-yellow-400">[警告] {payload.code}: {payload.message}</span>;

    default:
      return null;
  }
}

// Props 接口
interface Ai05Props {
  account: AccountInfo | null
  messages: ChatMessage[]
}

interface DemoMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  /** 额外的流式事件（如工具调用） */
  extra?: LlmChunkPayload[];
}

interface ToolCallState {
  index: number;
  id: string;
  name: string;
  arguments: string;
}

const INITIAL_MESSAGES: DemoMessage[] = [
  {
    id: "intro",
    role: "assistant",
    content:
      "在 **Tauri 壳** 中发送消息会通过 Rust 调用已配置的 LLM。请在 `src/lib/tauri-llm.ts` 里修改 **`LLM_LOCAL`**（`provider` / `model` / `temperature`）。浏览器里单独跑 Vite 预览时没有后端，会走本地占位回复。",
  },
];

const MOCK_FALLBACK =
  "当前不在 Tauri 环境，或请求失败。请在桌面应用内使用，并检查 Ollama / API Key 等配置。";

export default function Ai05({ account, messages: storeMessages }: Ai05Props) {
  // 本地消息状态，用于显示
  const [messages, setMessages] = useState<DemoMessage[]>(INITIAL_MESSAGES);
  const messagesRef = useRef<DemoMessage[]>(messages);
  messagesRef.current = messages;

  // 模型选择相关状态
  const [currentModel, setCurrentModel] = useState<AccountModelDto | null>(null);
  const [modelGroups, setModelGroups] = useState<ModelGroup[]>([]);
  const [isModelMenuOpen, setIsModelMenuOpen] = useState(false);
  const [isLoadingModel, setIsLoadingModel] = useState(false);

  // 加载当前模型
  useEffect(() => {
    if (!account) {
      setCurrentModel(null);
      return;
    }
    setIsLoadingModel(true);
    getChatModel(account.accountId)
      .then((model) => {
        setCurrentModel(model);
      })
      .catch((e) => {
        console.error("获取当前模型失败:", e);
        setCurrentModel(null);
      })
      .finally(() => {
        setIsLoadingModel(false);
      });
  }, [account]);

  // 加载所有模型列表
  useEffect(() => {
    getAllChatModels()
      .then((groups) => {
        setModelGroups(groups);
      })
      .catch((e) => {
        console.error("获取模型列表失败:", e);
        setModelGroups([]);
      });
  }, []);

  // 清空消息处理
  const handleClearMessages = async () => {
    if (!account) return;
    if (!confirm("确定要清空当前会话的所有消息吗？")) return;

    try {
      await clearMessages(account.accountId, "default");
      // 清空本地消息状态
      setMessages(INITIAL_MESSAGES);
    } catch (e) {
      console.error("清空消息失败:", e);
    }
  };

  // 当 store 中的消息变化时，更新本地消息
  useEffect(() => {
    if (!account) {
      // 未选择账号，显示欢迎消息
      setMessages(INITIAL_MESSAGES);
      return;
    }

    if (storeMessages.length > 0) {
      // 将后端消息转换为前端显示格式
      const converted = storeMessages.map((msg) => ({
        id: msg.id,
        role: msg.role as "user" | "assistant",
        content: msg.content,
      }));
      // 在欢迎消息后面追加历史消息
      setMessages([...INITIAL_MESSAGES, ...converted]);
    } else {
      // 新账号没有消息，只显示欢迎消息（清空旧消息）
      setMessages(INITIAL_MESSAGES);
    }
  }, [account, storeMessages]);

  const handleUserSubmit = async (text: string) => {
    if (!account) {
      alert('请先选择一个微信账号');
      return;
    }

    const userMessage: DemoMessage = {
      id: `user-${nanoid()}`,
      role: "user",
      content: text,
    };

    setMessages((prev) => [...prev, userMessage]);



    if (!isTauriRuntime()) {
      const stub: DemoMessage = {
        id: `assistant-${nanoid()}`,
        role: "assistant",
        content: MOCK_FALLBACK,
      };
      setMessages((prev) => [...prev, stub]);
      return;
    }

    const assistantId = `assistant-${nanoid()}`;
    const toolCallBuffer: Map<number, ToolCallState> = new Map();

    setMessages((prev) => [
      ...prev,
      { id: assistantId, role: "assistant", content: "", extra: [] },
    ]);

    try {
      await streamLlmChat({
        accountId: account.accountId,
        sessionId: "default",
        messages: [{ role: "user" as const, content: text }],
        onChunk: (payload) => {
          console.log("onChunk:", payload);

          switch (payload.kind) {
            case "text_delta":
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? { ...m, content: m.content + payload.text }
                    : m
                )
              );
              break;

            case "tool_call_start":
              toolCallBuffer.set(payload.index, {
                index: payload.index,
                id: payload.id,
                name: payload.name,
                arguments: "",
              });
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? {
                        ...m,
                        extra: [...(m.extra || []), payload],
                      }
                    : m
                )
              );
              break;

            case "tool_call_delta":
              const tc = toolCallBuffer.get(payload.index);
              if (tc) {
                tc.arguments += payload.arguments;
              }
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? {
                        ...m,
                        extra: [...(m.extra || []), payload],
                      }
                    : m
                )
              );
              break;

            case "tool_call_done":
              toolCallBuffer.delete(payload.index);
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? {
                        ...m,
                        extra: [...(m.extra || []), payload],
                      }
                    : m
                )
              );
              break;

            case "tool_result":
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? {
                        ...m,
                        extra: [...(m.extra || []), payload],
                      }
                    : m
                )
              );
              break;

            case "reasoning_delta":
            case "reference":
            case "audio_delta":
            case "usage":
            case "metadata":
            case "warning":
            case "error":
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? {
                        ...m,
                        extra: [...(m.extra || []), payload],
                      }
                    : m
                )
              );
              break;

            case "done":
              // done 事件不做额外处理
              break;
          }
        },
        onStreamError: (payload) => {
          console.log("onStreamError:", payload);
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantId
                ? { ...m, content: `**错误**\n\n${payload.message}` }
                : m
            )
          );
        },
        onStreamEnd: async () => {
          // LLM 流结束时，保存 assistant 回复到数据库

        },
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId
            ? { ...m, content: `**调用失败**\n\n${msg}` }
            : m
        )
      );
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col p-2">
      <div className="mx-auto flex min-h-0 flex-1 w-full flex-col overflow-hidden rounded-2xl border-border bg-card shadow-lg">
        <header className="flex shrink-0 items-center justify-between gap-4 border-b border-border/80 px-4 py-3">
          <div className="flex items-center gap-3">
            <div className="space-y-1">
              <div className="flex items-center gap-2 text-balance text-sm font-semibold">
                {account?.accountId || 'No Account Selected'}
              </div>
              <div className="flex items-center gap-2 text-pretty text-xs text-muted-foreground">
                <span className="inline-flex items-center gap-1">
                  <span className="size-1.5 rounded-full bg-emerald-500" />
                  {isTauriRuntime() ? "Tauri + LLM" : "浏览器预览"}
                </span>
                <span className="hidden sm:inline">- AI Elements</span>
              </div>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <Button
              size="icon"
              variant="ghost"
              className="size-8"
              aria-label="清空消息"
              title="清空消息"
              onClick={handleClearMessages}
              disabled={!account}
            >
              <IconTrash className="size-4" />
            </Button>
            {/* 模型选择按钮 */}
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-1.5 text-xs font-normal"
              onClick={() => setIsModelMenuOpen(true)}
              disabled={isLoadingModel || !account}
            >
              <IconAdjustmentsHorizontal className="size-3.5" />
              <span className="max-w-[100px] truncate">
                {isLoadingModel
                  ? "加载中..."
                  : currentModel?.display_name || currentModel?.model_name || "暂无模型"}
              </span>
            </Button>
          </div>
        </header>

        {/* 模型选择命令菜单 */}
        <Dialog open={isModelMenuOpen} onOpenChange={setIsModelMenuOpen}>
          <DialogContent
            className="gap-0 overflow-hidden rounded-xl border-border/50 p-0 shadow-lg sm:max-w-lg"
            showCloseButton={false}
          >
            <Command className="flex h-full w-full flex-col overflow-hidden bg-popover **:data-[slot=command-input-wrapper]:h-auto **:data-[slot=command-input-wrapper]:grow **:data-[slot=command-input-wrapper]:border-0 **:data-[slot=command-input-wrapper]:px-0">
              <div className="flex h-12 items-center gap-2 border-border/50 border-b px-4">
                <CommandInput
                  className="h-10 text-[15px]"
                  placeholder="搜索模型..."
                />
                <button
                  className="flex shrink-0 items-center"
                  onClick={() => setIsModelMenuOpen(false)}
                  type="button"
                >
                  <kbd className="rounded border bg-muted px-1.5 py-0.5 text-[10px] font-medium">Esc</kbd>
                </button>
              </div>

              <CommandList className="max-h-[400px] py-2">
                <CommandEmpty>暂无可用模型</CommandEmpty>

                {modelGroups.map((group) => (
                  <CommandGroup key={group.id} heading={group.name}>
                    {group.items.map((item) => (
                      <CommandItem
                        key={item.model_id}
                        className="mx-2 rounded-lg py-2.5"
                      // onSelect={() => handleModelSelect(group.id, item)}
                      >
                        {item.model_name}
                      </CommandItem>
                    ))}
                  </CommandGroup>
                ))}
              </CommandList>
            </Command>
          </DialogContent>
        </Dialog>

        <div className="min-h-0 flex-1 overflow-hidden">
          <Conversation className="h-full min-h-0 flex-1 bg-muted/30">
            <ConversationContent className="gap-6 ">
              {messages.map((message) => (
                <Message key={message.id} from={message.role}>
                  <MessageContent
                    className={cn(
                      "leading-relaxed",
                      message.role === "assistant" && "max-w-prose"
                    )}
                  >
                    {message.role === "assistant" ? (
                      <MessageResponse>{message.content}</MessageResponse>
                    ) : (
                      <p className="whitespace-pre-wrap text-pretty">
                        {message.content}
                      </p>
                    )}
                    {/* 显示额外的事件（如工具调用） */}
                    {message.extra && message.extra.length > 0 && (
                      <div className="mt-2 border-t border-border/50 pt-2 text-xs">
                        <p className="font-medium text-muted-foreground mb-1">事件记录:</p>
                        {message.extra.map((evt, i) => (
                          <div key={i} className="py-1 text-muted-foreground">
                            {renderChunkEvent(evt)}
                          </div>
                        ))}
                      </div>
                    )}
                  </MessageContent>
                </Message>
              ))}
            </ConversationContent>
            <ConversationScrollButton />
          </Conversation>
        </div>

        <ChatBox accountId={account?.accountId} onUserSubmit={handleUserSubmit} />
      </div>
    </div>
  );
}
