import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

type ModelConfigShellProps = {
  list: ReactNode;
  editor: ReactNode;
  className?: string;
};

export function ModelConfigShell({
  list,
  editor,
  className,
}: ModelConfigShellProps) {
  return (
    <div
      className={cn(
        "flex min-h-0 flex-1 gap-0 border-t border-border",
        className,
      )}
    >
      <div className="flex w-[min(100%,theme(spacing.72))] shrink-0 flex-col border-r border-border md:w-72">
        {list}
      </div>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">{editor}</div>
    </div>
  );
}
