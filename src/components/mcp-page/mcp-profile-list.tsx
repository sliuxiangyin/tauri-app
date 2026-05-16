import { Trash2Icon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { McpServeConfig } from "@/lib/mcp-serve-api";

type McpProfileListProps = {
  configs: McpServeConfig[];
  selectedId: number | null;
  onSelect: (id: number) => void;
  onDelete: (id: number) => void;
  onAdd: () => void;
};

export function McpProfileList({
  configs,
  selectedId,
  onSelect,
  onDelete,
  onAdd,
}: McpProfileListProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-0">
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-3">
        <span className="text-sm font-medium">MCP 服务</span>
        <Button type="button" size="sm" variant="default" onClick={onAdd}>
          添加
        </Button>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-1 p-2">
          {configs.length === 0 ? (
            <p className="px-2 py-6 text-center text-sm text-muted-foreground">
              暂无配置，点击「添加」创建。
            </p>
          ) : (
            configs.map((cfg) => (
              <div
                key={cfg.id}
                className={cn(
                  "flex w-full items-stretch gap-0.5 rounded-lg p-0.5",
                  selectedId === cfg.id && "bg-muted",
                )}
              >
                <Button
                  type="button"
                  variant="ghost"
                  className={cn(
                    "h-auto min-w-0 flex-1 flex-col items-stretch gap-1 rounded-md px-2.5 py-2 text-left font-normal whitespace-normal",
                  )}
                  onClick={() => onSelect(cfg.id)}
                >
                  <span className="truncate font-medium">{cfg.name}</span>
                  <span className="text-xs text-muted-foreground">
                    {cfg.config.transport === "http" ? "HTTP" : "STDIO"}
                  </span>
                </Button>
              </div>
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
