"use client";

import { useCallback, useRef, useState } from "react";
import type { ChatStatus } from "ai";
import {
  PromptInput,
  PromptInputButton,
  PromptInputFooter,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
} from "@/components/ai-elements/prompt-input";
import {
  IconBolt,
  IconMessageCircle,
  IconPaperclip,
  IconTool,
} from "@tabler/icons-react";
import { cancelLlmChat, isTauriRuntime } from "@/lib/api/tauri-llm";
import { cn } from "@/lib/utils";

const inputGroupClassName =
  "w-full [&>[data-slot=input-group]]:rounded-none [&>[data-slot=input-group]]:shadow-none [&>[data-slot=input-group]]:border-t [&>[data-slot=input-group]]:border-x-0 [&>[data-slot=input-group]]:border-b-0 [&>[data-slot=input-group]]:border-border/80 [&>[data-slot=input-group]]:focus-within:ring-0 [&>[data-slot=input-group]]:focus-within:ring-transparent [&>[data-slot=input-group]]:focus-within:ring-offset-0 [&>[data-slot=input-group]]:focus-within:border-border/80 [&>[data-slot=input-group]]:focus-within:outline-none";

export type ChatBoxProps = {
  accountId?: string;
  onUserSubmit?: (text: string) => void | Promise<void>;
  className?: string;
};

export function ChatBox({ accountId, onUserSubmit, className }: ChatBoxProps) {
  const [inputValue, setInputValue] = useState("");
  const [status, setStatus] = useState<ChatStatus>("ready");
  const accountIdRef = useRef(accountId);
  accountIdRef.current = accountId;

  const handleSubmit = useCallback(
    async (text: string) => {
      const trimmed = text.trim();
      if (!trimmed) {
        return;
      }

      setStatus("submitted");
      setInputValue("");

      try {
        setStatus("streaming");
        await Promise.resolve(onUserSubmit?.(trimmed));
      } finally {
        setStatus("ready");
      }
    },
    [onUserSubmit]
  );

  const handleCancel = useCallback(async () => {
    console.log("[ChatBox] 取消 LLM 流式响应")
    const accId = accountIdRef.current;
    if (accId && isTauriRuntime()) {
      console.log("[ChatBox] 取消 LLM 流式响应, accountId:", accId);
      await cancelLlmChat(accId);
    }
  }, []);

  return (
    <div className={cn("shrink-0 bg-background", className)}>
      <PromptInput
        onSubmit={(message) => void handleSubmit(message.text)}
        className={inputGroupClassName}
      >
        <PromptInputTextarea
          placeholder="Ask about the block, UI patterns, or an AI workflow"
          value={inputValue}
          onChange={(event) => setInputValue(event.currentTarget.value)}
        />
        <PromptInputFooter>
          <PromptInputTools>
            <PromptInputButton aria-label="Attach">
              <IconPaperclip className="size-4" />
            </PromptInputButton>
            <PromptInputButton aria-label="Quick prompt">
              <IconBolt className="size-4" />
            </PromptInputButton>
            <PromptInputButton aria-label="New chat">
              <IconMessageCircle className="size-4" />
            </PromptInputButton>
            <PromptInputButton aria-label="工具">
              <IconTool className="size-4" />
            </PromptInputButton>
          </PromptInputTools>
          <PromptInputSubmit
            status={status}
            disabled={status !== "streaming" && !inputValue.trim()}
            onClick={status === "streaming" ? handleCancel : undefined}
          />
        </PromptInputFooter>
      </PromptInput>
    </div>
  );
}
