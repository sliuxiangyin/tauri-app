"use client";

import { useCallback, useState } from "react";
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
import { cn } from "@/lib/utils";

const inputGroupClassName =
  "w-full [&>[data-slot=input-group]]:rounded-none [&>[data-slot=input-group]]:shadow-none [&>[data-slot=input-group]]:border-t [&>[data-slot=input-group]]:border-x-0 [&>[data-slot=input-group]]:border-b-0 [&>[data-slot=input-group]]:border-border/80 [&>[data-slot=input-group]]:focus-within:ring-0 [&>[data-slot=input-group]]:focus-within:ring-transparent [&>[data-slot=input-group]]:focus-within:ring-offset-0 [&>[data-slot=input-group]]:focus-within:border-border/80 [&>[data-slot=input-group]]:focus-within:outline-none";

export type ChatBoxProps = {
  onUserSubmit?: (text: string) => void | Promise<void>;
  className?: string;
};

export function ChatBox({ onUserSubmit, className }: ChatBoxProps) {
  const [inputValue, setInputValue] = useState("");
  const [status, setStatus] = useState<ChatStatus>("ready");

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
            disabled={!inputValue.trim() || status !== "ready"}
          />
        </PromptInputFooter>
      </PromptInput>
    </div>
  );
}
