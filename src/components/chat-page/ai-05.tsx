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
} from "@/components/ai-elements/message";
import { PlanStepsList } from "@/components/ai-elements/plan-steps-list";
import {
  ToolCallBlock,
  ToolResultBlock,
  ThinkingBlock,
} from "@/components/ai-elements/tool-call-block";
import { ChatBox } from "@/components/chat-page/chat-box";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  isTauriRuntime,
  streamLlmChat,
} from "@/lib/api/tauri-llm";
import {
  IconAdjustmentsHorizontal,
  IconTrash,
} from "@tabler/icons-react";
import { type MessageDto } from "@/stores/useChatStore";
import { type AccountInfo } from "@/lib/api/wechat";
import {
  clearMessages,
  type ContentBlockDto,
  type ContentItem,
} from "@/lib/api/messages";
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

// 渲染 ContentItem[] 的辅助函数
function renderContentItems(content: ContentItem[]): React.ReactNode {
  return content.map((item, index) => {
    if (item.type === "block") {
      const block = item.data;
      switch (block.block_type) {
        case "text":
          return (
            <p key={index} className="whitespace-pre-wrap text-pretty">
              {block.content}
            </p>
          );
        case "thinking":
          return <ThinkingBlock key={index} block={block} />;
        case "tool_call":
          return <ToolCallBlock key={index} block={block} />;
        case "tool_result":
          return <ToolResultBlock key={index} block={block} />;
        default:
          return (
            <p key={index} className="whitespace-pre-wrap text-pretty">
              {block.content}
            </p>
          );
      }
    }
    // Plan 类型暂不渲染
    return null;
  });
}

// Props 接口
interface Ai05Props {
  account: AccountInfo | null
  messages: MessageDto[]
}

interface DemoMessage {
  id: string;
  role: "user" | "assistant";
  /** 按 order_num 排序的内容块数组 */
  content: ContentItem[];
  /** Plan 步骤（用于展示） */
  planSteps?: import("@/lib/api/tauri-llm").PlanStepDto[];
  planReasoning?: string;
}

/** 流式过程中的临时块 */
interface StreamingBlock {
  block_type: string;
  order_num: number;
  content: string;
  thinking?: string;
  tool_name?: string;
  tool_arguments?: string;
  tool_result?: string;
  tool_status?: string;
}

/** 工具调用缓冲区（用于聚合增量参数） */
interface ToolCallBuffer {
  index: number;
  id: string;
  name: string;
  arguments: string;
}

const INITIAL_MESSAGES: DemoMessage[] = [
  {
    id: "intro",
    role: "assistant",
    content: [
      {
        type: "block",
        data: {
          id: "intro-block",
          mid: "intro",
          block_type: "text",
          order_num: 0,
          source: "system",
          content:
            "在 **Tauri 壳** 中发送消息会通过 Rust 调用已配置的 LLM。请在 `src/lib/tauri-llm.ts` 里修改 **`LLM_LOCAL`**（`provider` / `model` / `temperature`）。浏览器里单独跑 Vite 预览时没有后端，会走本地占位回复。",
          extends: "",
          metadata: "",
          created_at: "",
        } satisfies ContentBlockDto,
      },
    ],
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
    console.log("storeMessages:", storeMessages);
    if (storeMessages.length > 0) {
      // 直接使用后端返回的 content（已经是 ContentItem[]）
      const converted: DemoMessage[] = storeMessages.map((msg) => ({
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

    // 创建 user 消息（content 为单个 text block）
    const userMessage: DemoMessage = {
      id: `user-${nanoid()}`,
      role: "user",
      content: [
        {
          type: "block",
          data: {
            id: `user-block-${nanoid(8)}`,
            mid: "",
            block_type: "text",
            order_num: 0,
            source: "user",
            content: text,
            extends: "",
            metadata: "",
            created_at: new Date().toISOString(),
          } satisfies ContentBlockDto,
        },
      ],
    };

    setMessages((prev) => [...prev, userMessage]);

    if (!isTauriRuntime()) {
      const stub: DemoMessage = {
        id: `assistant-${nanoid()}`,
        role: "assistant",
        content: [
          {
            type: "block",
            data: {
              id: `stub-block-${nanoid(8)}`,
              mid: "",
              block_type: "text",
              order_num: 0,
              source: "system",
              content: MOCK_FALLBACK,
              extends: "",
              metadata: "",
              created_at: new Date().toISOString(),
            } satisfies ContentBlockDto,
          },
        ],
      };
      setMessages((prev) => [...prev, stub]);
      return;
    }

    const assistantId = `assistant-${nanoid()}`;

    // 流式处理状态
    const streamingBlocks: StreamingBlock[] = [];
    const toolBuffer: Map<string, ToolCallBuffer> = new Map();

    // 创建 assistant 占位消息
    setMessages((prev) => [
      ...prev,
      {
        id: assistantId,
        role: "assistant",
        content: [],
      },
    ]);

    // 辅助函数：将 StreamingBlock 转换为 ContentItem
    const blockToContentItem = (block: StreamingBlock, mid: string): ContentItem => {
      const baseData = {
        id: `${mid}-block-${block.order_num}`,
        mid,
        block_type: block.block_type as ContentBlockDto["block_type"],
        order_num: block.order_num,
        source: "chat",
        extends: "",
        metadata: "",
        created_at: new Date().toISOString(),
      };

      switch (block.block_type) {
        case "text":
          return {
            type: "block",
            data: {
              ...baseData,
              content: block.content,
            } satisfies ContentBlockDto,
          };
        case "thinking":
          return {
            type: "block",
            data: {
              ...baseData,
              thinking: block.content,
            } satisfies ContentBlockDto,
          };
        case "tool_call":
          return {
            type: "block",
            data: {
              ...baseData,
              tool_name: block.tool_name,
              tool_arguments: block.tool_arguments,
            } satisfies ContentBlockDto,
          };
        case "tool_result":
          return {
            type: "block",
            data: {
              ...baseData,
              tool_name: block.tool_name,
              tool_output: block.tool_result,
              tool_status: block.tool_status,
            } satisfies ContentBlockDto,
          };
        default:
          return {
            type: "block",
            data: {
              ...baseData,
              content: block.content,
            } satisfies ContentBlockDto,
          };
      }
    };

    // 更新 assistant 消息的 content
    const updateAssistantContent = (blocks: StreamingBlock[]) => {
      const contentItems: ContentItem[] = blocks.map((b) =>
        blockToContentItem(b, assistantId)
      );
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId ? { ...m, content: contentItems } : m
        )
      );
    };

    try {
      await streamLlmChat({
        accountId: account.accountId,
        sessionId: "default",
        messages: [{ role: "user" as const, content: text }],
        onChunk: (payload) => {
          console.log("onChunk:", payload);
          switch (payload.kind) {
            case "block_start":
              // 创建新块
              streamingBlocks.push({
                block_type: payload.block_type,
                order_num: payload.order_num,
                content: "",
              });
              break;

            case "text_delta":
              // 累加到当前 text block
              if (streamingBlocks.length > 0) {
                const current = streamingBlocks[streamingBlocks.length - 1];
                if (current.block_type === "text") {
                  current.content += payload.text;
                  updateAssistantContent(streamingBlocks);
                }
              }
              break;

            case "reasoning_delta":
              // 累加到当前 thinking block
              if (streamingBlocks.length > 0) {
                const current = streamingBlocks[streamingBlocks.length - 1];
                if (current.block_type === "thinking") {
                  current.content += payload.text;
                } else {
                  // 如果当前不是 thinking，创建新的 thinking block
                  streamingBlocks.push({
                    block_type: "thinking",
                    order_num: streamingBlocks.length,
                    content: payload.text,
                  });
                }
                updateAssistantContent(streamingBlocks);
              }
              break;

            case "tool_call_start":
              // 记录到 toolBuffer（使用 index 作为 key）
              toolBuffer.set(String(payload.index), {
                index: payload.index,
                id: payload.id,
                name: payload.name,
                arguments: "",
              });
              break;

            case "tool_call_delta":
              // 累加参数（使用 index 作为 key）
              const tb = toolBuffer.get(String(payload.index));
              if (tb) {
                tb.arguments += payload.arguments;
              }
              break;

            case "tool_call_done":
              // 合并到当前 block（使用 index 作为 key）
              const completed = toolBuffer.get(String(payload.index));
              if (completed && streamingBlocks.length > 0) {
                const current = streamingBlocks[streamingBlocks.length - 1];
                current.tool_name = completed.name;
                current.tool_arguments = completed.arguments;
                updateAssistantContent(streamingBlocks);
              }
              toolBuffer.delete(String(payload.index));
              break;

            case "tool_result":
              // 更新当前 block
              if (streamingBlocks.length > 0) {
                const current = streamingBlocks[streamingBlocks.length - 1];
                current.tool_name = payload.name;
                current.tool_result = JSON.stringify(payload.result);
                current.tool_status = payload.success ? "success" : "failed";
                updateAssistantContent(streamingBlocks);
              }
              break;

            case "plan_steps":
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? {
                        ...m,
                        planSteps: payload.steps,
                        planReasoning: payload.reasoning,
                      }
                    : m
                )
              );
              break;

            case "done":
              // done 事件不做额外处理（内容已在各 delta 事件中累加）
              break;

            case "reference":
            case "audio_delta":
            case "usage":
            case "metadata":
            case "warning":
            case "error":
              // 这些事件暂不处理
              break;
          }
        },
        onStreamError: (payload) => {
          console.log("onStreamError:", payload);
          // 添加错误 block
          streamingBlocks.push({
            block_type: "text",
            order_num: streamingBlocks.length,
            content: `**错误**\n\n${payload.message}`,
          });
          updateAssistantContent(streamingBlocks);
        },
        onStreamEnd: async () => {
          // LLM 流结束，后端已入库，前端只需更新显示
        },
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      streamingBlocks.push({
        block_type: "text",
        order_num: streamingBlocks.length,
        content: `**调用失败**\n\n${msg}`,
      });
      updateAssistantContent(streamingBlocks);
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
                    {/* 渲染 content 数组 */}
                    {renderContentItems(message.content)}
                    {/* Plan 步骤列表 */}
                    {message.planSteps && message.planSteps.length > 0 && (
                      <PlanStepsList
                        steps={message.planSteps}
                        reasoning={message.planReasoning}
                      />
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
