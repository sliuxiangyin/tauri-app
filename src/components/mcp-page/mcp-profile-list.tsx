import { Trash2Icon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { McpServeConfig } from "@/lib/mcp-serve-api";
import { ItemLink } from "./mcp-profile-item";

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
              
              <ItemLink key={cfg.id} cfg={cfg} onSelect={onSelect}  />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
