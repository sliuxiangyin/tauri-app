"use client";

import { useRef, useState } from "react";
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
import { ChatBox } from "@/components/chat/chat-box";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  isTauriRuntime,
  LLM_LOCAL,
  streamLlmChat,
  type ChatMessagePayload,
} from "@/lib/tauri-llm";
import {
  IconAdjustmentsHorizontal,
  IconRefresh,
} from "@tabler/icons-react";

interface DemoMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
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

export default function Ai05() {
  const [messages, setMessages] = useState<DemoMessage[]>(INITIAL_MESSAGES);
  const messagesRef = useRef<DemoMessage[]>(messages);
  messagesRef.current = messages;

  const handleUserSubmit = async (text: string) => {
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
    setMessages((prev) => [
      ...prev,
      { id: assistantId, role: "assistant", content: "" },
    ]);

    const history: ChatMessagePayload[] = messagesRef.current
      .filter((m) => m.role === "user" || m.role === "assistant")
      .map((m) => ({ role: m.role, content: m.content }));

    history.push({ role: "user", content: text });

    const streamId = crypto.randomUUID();
    const provider = LLM_LOCAL.provider;

    try {
      await streamLlmChat({
        streamId,
        provider,
        req: {
          messages: history,
          model: LLM_LOCAL.model,
          temperature: LLM_LOCAL.temperature,
        },
        onChunk: (payload) => {
          if (payload.kind === "text_delta") {
            setMessages((prev) =>
              prev.map((m) =>
                m.id === assistantId
                  ? { ...m, content: m.content + payload.text }
                  : m
              )
            );
          }
        },
        onStreamError: (payload) => {
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantId
                ? { ...m, content: `**错误**\n\n${payload.message}` }
                : m
            )
          );
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
                Documenso Chat
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
              aria-label="Refresh"
              title="Refresh"
            >
              <IconRefresh className="size-4" />
            </Button>
            <Button
              size="icon"
              variant="ghost"
              className="size-8"
              aria-label="Settings"
              title="Settings"
            >
              <IconAdjustmentsHorizontal className="size-4" />
            </Button>
          </div>
        </header>

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
                  </MessageContent>
                </Message>
              ))}
            </ConversationContent>
            <ConversationScrollButton />
          </Conversation>
        </div>

        <ChatBox onUserSubmit={handleUserSubmit} />
      </div>
    </div>
  );
}
