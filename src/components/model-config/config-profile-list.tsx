import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";

import { ConfigProfileListItem } from "./config-profile-list-item";
import type { ModelConfigProfile } from "./types";

type ConfigProfileListProps = {
  profiles: ModelConfigProfile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onDeleteProfile: (id: string) => void;
};

export function ConfigProfileList({
  profiles,
  selectedId,
  onSelect,
  onAdd,
  onDeleteProfile,
}: ConfigProfileListProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-0">
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-3">
        <span className="text-sm font-medium">配置列表</span>
        <Button type="button" size="sm" variant="default" onClick={onAdd}>
          添加
        </Button>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-1 p-2">
          {profiles.length === 0 ? (
            <p className="px-2 py-6 text-center text-sm text-muted-foreground">
              暂无配置，点击「添加」创建。
            </p>
          ) : (
            profiles.map((p) => (
              <ConfigProfileListItem
                key={p.id}
                profile={p}
                selected={p.id === selectedId}
                onSelect={() => onSelect(p.id)}
                onDelete={() => onDeleteProfile(p.id)}
              />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
