import { Trash2Icon } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import type { ModelConfigProfile, ProviderKind } from "./types";
import { listItemLabel } from "./types";

const providerLabel: Record<ProviderKind, string> = {
  open_ai: "OpenAI 兼容",
  ollama: "Ollama",
};

type ProfileListItemProps = {
  profile: ModelConfigProfile;
  selected: boolean;
  onSelect: (id:string) => void;
  onDelete: (id:string) => void;
};

export function ProfileListItem({
  profile,
  selected,
  onSelect,
  onDelete,
}: ProfileListItemProps) {
  return (
    <div
      className={cn(
        "flex w-full items-stretch gap-0.5 rounded-lg p-0.5",
        selected && "bg-muted",
      )}
    >
      <Button
        type="button"
        variant="ghost"
        className={cn(
          "h-auto min-w-0 flex-1 flex-col items-stretch gap-1 rounded-md px-2.5 py-2 text-left font-normal whitespace-normal",
        )}
        onClick={() => onSelect(profile.id)}
      >
        <span className="truncate font-medium">{listItemLabel(profile)}</span>
        <div className="flex flex-wrap items-center gap-1.5">
          <Badge variant="secondary" className="text-xs font-normal">
            {providerLabel[profile.provider]}
          </Badge>
          {!profile.enabled ? (
            <span className="text-xs text-muted-foreground">已关闭</span>
          ) : null}
        </div>
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        className="shrink-0 self-center text-muted-foreground hover:text-destructive"
        onClick={() => onDelete(profile.id)}
        aria-label="删除此配置"
      >
        <Trash2Icon />
      </Button>
    </div>
  );
}
