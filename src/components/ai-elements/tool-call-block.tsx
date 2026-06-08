"use client";

import { cn } from "@/lib/utils";
import type { ContentBlockDto } from "@/lib/api/messages";
import { ChevronDown, ChevronRight, Loader2 } from "lucide-react";
import { useState } from "react";

interface ToolCallBlockProps {
  block: ContentBlockDto;
  className?: string;
}

/** 工具调用块 */
export function ToolCallBlock({ block, className }: ToolCallBlockProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  // 解析参数
  let parsedArgs: Record<string, unknown> = {};
  if (block.tool_arguments) {
    try {
      parsedArgs = JSON.parse(block.tool_arguments);
    } catch {
      parsedArgs = { raw: block.tool_arguments };
    }
  }

  return (
    <div className={cn("rounded-lg border border-orange-500/30 bg-orange-500/5 p-3 text-sm", className)}>
      <button
        type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-2 text-left"
      >
        <span className="text-orange-500">
          {isExpanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        </span>
        <span className="text-xs font-medium text-orange-500">🔧 工具调用</span>
        <span className="font-medium">{block.tool_name}</span>
      </button>

      {isExpanded && block.tool_arguments && (
        <div className="mt-2 rounded bg-muted p-2">
          <p className="mb-1 text-xs text-muted-foreground">参数:</p>
          <pre className="overflow-x-auto text-xs">
            {JSON.stringify(parsedArgs, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

interface ToolResultBlockProps {
  block: ContentBlockDto;
  className?: string;
}

/** 工具结果块 */
export function ToolResultBlock({ block, className }: ToolResultBlockProps) {
  const [isExpanded, setIsExpanded] = useState(true);
  const isSuccess = block.tool_status === "success";

  // 解析输出
  let parsedOutput: unknown = null;
  if (block.tool_output) {
    try {
      parsedOutput = JSON.parse(block.tool_output);
    } catch {
      parsedOutput = block.tool_output;
    }
  }

  return (
    <div
      className={cn(
        "rounded-lg border p-3 text-sm",
        isSuccess
          ? "border-green-500/30 bg-green-500/5"
          : "border-red-500/30 bg-red-500/5",
        className
      )}
    >
      <button
        type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-2 text-left"
      >
        <span className={isSuccess ? "text-green-500" : "text-red-500"}>
          {isExpanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        </span>
        <span className={cn("text-xs font-medium", isSuccess ? "text-green-500" : "text-red-500")}>
          {isSuccess ? "✅ 成功" : "❌ 失败"}
        </span>
        {block.tool_name && <span className="font-medium">{block.tool_name}</span>}
      </button>

      {isExpanded && (
        <div className="mt-2">
          {block.tool_error ? (
            <p className="rounded bg-red-500/10 p-2 text-xs text-red-500">
              {block.tool_error}
            </p>
          ) : block.tool_output ? (
            <div className="rounded bg-muted p-2">
              <p className="mb-1 text-xs text-muted-foreground">结果:</p>
              <pre className="max-h-48 overflow-auto text-xs">
                {typeof parsedOutput === "string"
                  ? parsedOutput
                  : JSON.stringify(parsedOutput, null, 2)}
              </pre>
            </div>
          ) : isSuccess ? (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="size-3 animate-spin" />
              等待结果...
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}

/** 工具调用块（统一模型：包含调用信息 + 执行结果） */
interface ToolInvocationBlockProps {
  block: ContentBlockDto;
  className?: string;
}

/**
 * 工具调用卡片（统一显示）
 * - 工具名称
 * - 输入参数
 * - 执行结果/状态
 * 兼容旧数据：tool_call / tool_result 也会路由到此组件
 */
export function ToolInvocationBlock({ block, className }: ToolInvocationBlockProps) {
  const [isExpanded, setIsExpanded] = useState(true);
  const isSuccess = block.tool_status === "success";
  const isPending = block.tool_status === "pending" || !block.tool_status;

  // 解析参数
  let parsedArgs: Record<string, unknown> = {};
  if (block.tool_arguments) {
    try {
      parsedArgs = JSON.parse(block.tool_arguments);
    } catch {
      parsedArgs = { raw: block.tool_arguments };
    }
  }

  // 解析输出
  let parsedOutput: unknown = null;
  if (block.tool_output) {
    try {
      parsedOutput = JSON.parse(block.tool_output);
    } catch {
      parsedOutput = block.tool_output;
    }
  }

  // 获取状态颜色和标签
  const getStatusColor = () => {
    if (isPending) return "text-orange-500";
    return isSuccess ? "text-green-500" : "text-red-500";
  };

  const getStatusLabel = () => {
    if (isPending) return "⏳ 执行中";
    return isSuccess ? "✅ 成功" : "❌ 失败";
  };

  const getBorderColor = () => {
    if (isPending) return "border-orange-500/30 bg-orange-500/5";
    return isSuccess ? "border-green-500/30 bg-green-500/5" : "border-red-500/30 bg-red-500/5";
  };

  return (
    <div className={cn("rounded-lg border p-3 text-sm", getBorderColor(), className)}>
      <button
        type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-2 text-left"
      >
        <span className={getStatusColor()}>
          {isExpanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        </span>
        <span className="text-xs font-medium text-orange-500">🔧</span>
        <span className="font-medium">{block.tool_name || "未知工具"}</span>
        <span className={cn("text-xs", getStatusColor())}>{getStatusLabel()}</span>
        {block.tool_duration_ms && (
          <span className="text-xs text-muted-foreground">({block.tool_duration_ms}ms)</span>
        )}
      </button>

      {isExpanded && (
        <div className="mt-2 space-y-2">
          {/* 输入参数 */}
          {block.tool_arguments && (
            <div className="rounded bg-muted p-2">
              <p className="mb-1 text-xs text-muted-foreground">输入参数:</p>
              <pre className="overflow-x-auto text-xs">
                {JSON.stringify(parsedArgs, null, 2)}
              </pre>
            </div>
          )}

          {/* 输出结果 */}
          {isPending ? (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="size-3 animate-spin" />
              等待执行结果...
            </div>
          ) : block.tool_error ? (
            <div className="rounded bg-red-500/10 p-2">
              <p className="mb-1 text-xs text-muted-foreground">错误:</p>
              <p className="text-xs text-red-500">{block.tool_error}</p>
            </div>
          ) : block.tool_output ? (
            <div className="rounded bg-muted p-2">
              <p className="mb-1 text-xs text-muted-foreground">输出结果:</p>
              <pre className="max-h-48 overflow-auto text-xs">
                {typeof parsedOutput === "string"
                  ? parsedOutput
                  : JSON.stringify(parsedOutput, null, 2)}
              </pre>
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}

interface ThinkingBlockProps {
  block: ContentBlockDto;
  className?: string;
}

/** 思考过程块 */
export function ThinkingBlock({ block, className }: ThinkingBlockProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const content = block.thinking || block.content || "";

  // 如果内容较短，直接显示
  if (content.length < 200) {
    return (
      <div className={cn("rounded-lg bg-blue-500/5 p-3 text-sm", className)}>
        <p className="mb-1 text-xs font-medium text-blue-500">💭 思考过程</p>
        <p className="whitespace-pre-wrap text-pretty text-muted-foreground">
          {content}
        </p>
      </div>
    );
  }

  return (
    <div className={cn("rounded-lg bg-blue-500/5 p-3 text-sm", className)}>
      <button
        type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-2 text-left"
      >
        <span className="text-blue-500">
          {isExpanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        </span>
        <span className="text-xs font-medium text-blue-500">💭 思考过程</span>
        <span className="text-xs text-muted-foreground">
          ({content.length} 字符)
        </span>
      </button>

      {isExpanded && (
        <p className="mt-2 whitespace-pre-wrap text-pretty text-muted-foreground">
          {content}
        </p>
      )}
    </div>
  );
}