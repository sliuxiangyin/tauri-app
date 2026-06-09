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
  ToolInvocationBlock,
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
  type PlanDto,
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
        case "tool":
          return <ToolInvocationBlock key={index} block={block} />;
        default:
          return (
            <p key={index} className="whitespace-pre-wrap text-pretty">
              {block.content}
            </p>
          );
      }
    }
    if (item.type === "plan") {
      const plan = item.data;
      // 将 steps JSON 字符串解析为 PlanStepDto[]
      let steps: import("@/lib/api/tauri-llm").PlanStepDto[] = [];
      if (plan.steps) {
        try {
          steps = JSON.parse(plan.steps);
        } catch {
          // steps 解析失败则不渲染
        }
      }
      return (
        <PlanStepsList
          key={index}
          steps={steps}
          reasoning={plan.reasoning}
        />
      );
    }
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

    // 创建 assistant 占位消息
    setMessages((prev) => [
      ...prev,
      {
        id: assistantId,
        role: "assistant",
        content: [],
      },
    ]);

    // 工具参数缓冲区（用于聚合增量参数）
    const toolArgumentsBuffer = new Map<string, string>();

    try {
      await streamLlmChat({
        accountId: account.accountId,
        sessionId: "default",
        messages: [{ role: "user" as const, content: text }],
        onChunk: (payload) => {
          console.log("onChunk:", payload);
          switch (payload.kind) {
            // ========================================
            // block_start: 立即插入 ContentItem（根据 block_type 初始化对应字段）
            // ========================================
            case "block_start":
              {
                const baseData = {
                  id: `${assistantId}-block-${payload.order_num}`,
                  mid: assistantId,
                  block_type: payload.block_type as ContentBlockDto["block_type"],
                  order_num: payload.order_num,
                  source: "chat",
                  extends: "",
                  metadata: "",
                  created_at: new Date().toISOString(),
                };

                let blockData: ContentBlockDto;
                switch (payload.block_type) {
                  case "text":
                    blockData = { ...baseData, content: "" } as ContentBlockDto;
                    break;
                  case "thinking":
                    blockData = { ...baseData, thinking: "" } as ContentBlockDto;
                    break;
                  case "tool":
                    blockData = {
                      ...baseData,
                      tool_name: "",
                      tool_arguments: "",
                      tool_status: "pending",
                    } as ContentBlockDto;
                    break;
                  default:
                    blockData = { ...baseData, content: "" } as ContentBlockDto;
                }

                const blockItem: ContentItem = { type: "block", data: blockData };
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    return { ...m, content: [...m.content, blockItem] };
                  })
                );
              }
              break;

            // ========================================
            // text_delta: 累加到最近插入的 text block
            // ========================================
            case "text_delta":
              {
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    // 找到最近插入的 text block 的索引
                    let lastTextIndex = -1;
                    for (let i = m.content.length - 1; i >= 0; i--) {
                      const item = m.content[i];
                      if (item.type === "block" && (item.data as ContentBlockDto).block_type === "text") {
                        lastTextIndex = i;
                        break;
                      }
                    }
                    if (lastTextIndex === -1) return m;
                    const newContent = m.content.map((item, i): ContentItem => {
                      if (i === lastTextIndex && item.type === "block") {
                        const blockData = item.data as ContentBlockDto;
                        return {
                          type: "block",
                          data: {
                            ...blockData,
                            content: (blockData.content ?? "") + payload.text,
                          },
                        };
                      }
                      return item;
                    });
                    return { ...m, content: newContent };
                  })
                );
              }
              break;

            // ========================================
            // reasoning_delta: 累加到最近插入的 thinking block
            // ========================================
            case "reasoning_delta":
              {
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    // 找到最近插入的 thinking block 的索引
                    let lastThinkingIndex = -1;
                    for (let i = m.content.length - 1; i >= 0; i--) {
                      const item = m.content[i];
                      if (item.type === "block" && (item.data as ContentBlockDto).block_type === "thinking") {
                        lastThinkingIndex = i;
                        break;
                      }
                    }
                    if (lastThinkingIndex === -1) return m;
                    const newContent = m.content.map((item, i): ContentItem => {
                      if (i === lastThinkingIndex && item.type === "block") {
                        const blockData = item.data as ContentBlockDto;
                        return {
                          type: "block",
                          data: {
                            ...blockData,
                            thinking: (blockData.thinking ?? "") + payload.text,
                          },
                        };
                      }
                      return item;
                    });
                    return { ...m, content: newContent };
                  })
                );
              }
              break;

            // ========================================
            // tool_call_start: 复用 block_start 已插入的 tool block，补充 tool_name
            // ========================================
            case "tool_call_start":
              {
                // 不插入新 block，而是更新 block_start 已创建的 tool block 的 tool_name
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    // 找到 order_num 匹配 payload.index 的 tool block，或者最近的空 tool block
                    let targetIndex = -1;
                    for (let i = m.content.length - 1; i >= 0; i--) {
                      const item = m.content[i];
                      if (
                        item.type === "block" &&
                        (item.data as ContentBlockDto).block_type === "tool" &&
                        !(item.data as ContentBlockDto).tool_name
                      ) {
                        targetIndex = i;
                        break;
                      }
                    }
                    if (targetIndex === -1) return m;
                    const newContent = m.content.map((item, i): ContentItem => {
                      if (i === targetIndex && item.type === "block") {
                        return {
                          type: "block",
                          data: {
                            ...item.data,
                            tool_name: payload.name,
                          },
                        };
                      }
                      return item;
                    });
                    return { ...m, content: newContent };
                  })
                );
              }
              break;

            // ========================================
            // tool_call_delta: 累加参数（暂存，不更新 UI）
            // ========================================
            case "tool_call_delta":
              {
                // 参数暂存到本地 Map，后续 tool_call_done 时一起更新
                // 注意：这里暂不更新 UI，因为 block 数据在 tool_call_start 时已插入
                const tb = toolArgumentsBuffer.get(String(payload.index));
                if (tb !== undefined) {
                  toolArgumentsBuffer.set(String(payload.index), tb + payload.arguments);
                } else {
                  toolArgumentsBuffer.set(String(payload.index), payload.arguments);
                }
              }
              break;

            // ========================================
            // tool_call_done: 合并参数到最近插入的 tool block
            // ========================================
            case "tool_call_done":
              {
                const args = toolArgumentsBuffer.get(String(payload.index)) ?? "";
                toolArgumentsBuffer.delete(String(payload.index));

                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    // 找到最近插入的 tool block 的索引
                    let lastToolIndex = -1;
                    for (let i = m.content.length - 1; i >= 0; i--) {
                      const item = m.content[i];
                      if (item.type === "block" && (item.data as ContentBlockDto).block_type === "tool") {
                        lastToolIndex = i;
                        break;
                      }
                    }
                    if (lastToolIndex === -1) return m;
                    const newContent = m.content.map((item, i): ContentItem => {
                      if (i === lastToolIndex && item.type === "block") {
                        const blockData = item.data as ContentBlockDto;
                        const argsStr = typeof args === "string" ? args : JSON.stringify(args);
                        return {
                          type: "block",
                          data: {
                            ...blockData,
                            tool_arguments: argsStr,
                          },
                        };
                      }
                      return item;
                    });
                    return { ...m, content: newContent };
                  })
                );
              }
              break;

            // ========================================
            // tool_result: 更新 tool block 的执行结果
            // ========================================
            case "tool_result":
              {
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    const newContent = m.content.map((item) => {
                      if (
                        item.type === "block" &&
                        (item.data as ContentBlockDto).tool_name === payload.name
                      ) {
                        return {
                          ...item,
                          data: {
                            ...item.data,
                            tool_output: JSON.stringify(payload.result),
                            tool_status: payload.success ? "success" : "failed",
                          } satisfies ContentBlockDto,
                        };
                      }
                      return item;
                    });
                    return { ...m, content: newContent };
                  })
                );
              }
              break;

            // ========================================
            // plan_start: 立即插入 plan ContentItem
            // ========================================
            case "plan_start":
              {
                const planItem: ContentItem = {
                  type: "plan",
                  data: {
                    id: payload.plan_id,
                    mid: assistantId,
                    need_agent: "1",
                    order_num: payload.order_num,
                    reasoning: "",
                    steps: "[]",
                    step_results: undefined,
                    stop_reason: undefined,
                    completed_at: undefined,
                    created_at: new Date().toISOString(),
                  } satisfies PlanDto,
                };
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    const newContent = [...m.content, planItem];
                    return { ...m, content: newContent };
                  })
                );
              }
              break;

            // ========================================
            // plan_steps: 通过 plan_id 更新 reasoning 和 steps
            // ========================================
            case "plan_steps":
              {
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    const newContent = m.content.map((item) => {
                      if (item.type === "plan" && item.data.id === payload.plan_id) {
                        return {
                          ...item,
                          data: {
                            ...item.data,
                            reasoning: payload.reasoning,
                            steps: JSON.stringify(payload.steps),
                          } satisfies PlanDto,
                        };
                      }
                      return item;
                    });
                    return { ...m, content: newContent };
                  })
                );
              }
              break;

            // ========================================
            // plan_update: 通过 plan_id 更新 step_results 和 stop_reason
            // ========================================
            case "plan_update":
              {
                setMessages((prev) =>
                  prev.map((m) => {
                    if (m.id !== assistantId) return m;
                    const newContent = m.content.map((item) => {
                      if (item.type === "plan" && item.data.id === payload.plan_id) {
                        return {
                          ...item,
                          data: {
                            ...item.data,
                            step_results:
                              payload.step_results ?? item.data.step_results,
                            stop_reason: payload.stop_reason,
                          } satisfies PlanDto,
                        };
                      }
                      return item;
                    });
                    return { ...m, content: newContent };
                  })
                );
              }
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
          // 插入错误 block
          const errorItem: ContentItem = {
            type: "block",
            data: {
              id: `error-${Date.now()}`,
              mid: assistantId,
              block_type: "text",
              order_num: 9999,
              source: "system",
              content: `**错误**\n\n${payload.message}`,
              extends: "",
              metadata: "",
              created_at: new Date().toISOString(),
            } satisfies ContentBlockDto,
          };
          setMessages((prev) =>
            prev.map((m) => {
              if (m.id !== assistantId) return m;
              return { ...m, content: [...m.content, errorItem] };
            })
          );
        },
        onStreamEnd: async () => {
          // LLM 流结束，后端已入库，前端只需更新显示
        },
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      // 插入错误 block
      const errorItem: ContentItem = {
        type: "block",
        data: {
          id: `error-${Date.now()}`,
          mid: assistantId,
          block_type: "text",
          order_num: 9999,
          source: "system",
          content: `**调用失败**\n\n${msg}`,
          extends: "",
          metadata: "",
          created_at: new Date().toISOString(),
        } satisfies ContentBlockDto,
      };
      setMessages((prev) =>
        prev.map((m) => {
          if (m.id !== assistantId) return m;
          return { ...m, content: [...m.content, errorItem] };
        })
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
                    {/* 渲染 content 数组（blocks + plan 统一渲染） */}
                    {renderContentItems(message.content)}
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
