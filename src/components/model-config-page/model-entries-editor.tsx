import { PlusIcon, Trash2Icon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

import type { ModelEntry } from "./types";
import { createEmptyModelEntry } from "./types";

type ModelEntriesEditorProps = {
  models: ModelEntry[];
  onChange: (models: ModelEntry[]) => void;
};

export function ModelEntriesEditor({
  models,
  onChange,
}: ModelEntriesEditorProps) {
  const updateRow = (id: string, patch: Partial<ModelEntry>) => {
    onChange(
      models.map((m) => (m.id === id ? { ...m, ...patch } : m)),
    );
  };

  const removeRow = (id: string) => {
    onChange(models.filter((m) => m.id !== id));
  };

  const addRow = () => {
    onChange([...models, createEmptyModelEntry()]);
  };

  return (
    <div className="flex flex-col gap-3">
      <Label className="text-sm font-medium">模型列表</Label>
      <div className="rounded-lg border border-border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="min-w-[7rem]">
                <span className="inline-flex items-center gap-0.5">
                  模型 ID
                  <span className="text-destructive" aria-hidden>
                    *
                  </span>
                </span>
              </TableHead>
              <TableHead className="min-w-[7rem]">
                <span className="inline-flex items-center gap-0.5">
                  模型名称
                  <span className="text-destructive" aria-hidden>
                    *
                  </span>
                </span>
              </TableHead>
              <TableHead className="min-w-[7rem]">分组名称</TableHead>
              <TableHead className="w-12 text-right"> </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {models.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4} className="text-center text-muted-foreground">
                  暂无模型行，点击下方按钮添加。
                </TableCell>
              </TableRow>
            ) : (
              models.map((m) => {
                const idInvalid = m.modelId.trim().length === 0;
                const nameInvalid = m.modelName.trim().length === 0;
                return (
                <TableRow key={m.id}>
                  <TableCell>
                    <Input
                      value={m.modelId}
                      onChange={(e) =>
                        updateRow(m.id, { modelId: e.target.value })
                      }
                      placeholder="model-id"
                      className="h-8"
                      required
                      aria-invalid={idInvalid}
                      aria-label="模型 ID（必填）"
                    />
                  </TableCell>
                  <TableCell>
                    <Input
                      value={m.modelName}
                      onChange={(e) =>
                        updateRow(m.id, { modelName: e.target.value })
                      }
                      placeholder="显示名称"
                      className="h-8"
                      required
                      aria-invalid={nameInvalid}
                      aria-label="模型名称（必填）"
                    />
                  </TableCell>
                  <TableCell>
                    <Input
                      value={m.groupName}
                      onChange={(e) =>
                        updateRow(m.id, { groupName: e.target.value })
                      }
                      placeholder="分组"
                      className="h-8"
                    />
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      type="button"
                      size="icon-sm"
                      variant="ghost"
                      className="text-muted-foreground hover:text-destructive"
                      onClick={() => removeRow(m.id)}
                      aria-label="删除此行"
                    >
                      <Trash2Icon />
                    </Button>
                  </TableCell>
                </TableRow>
                );
              })
            )}
          </TableBody>
        </Table>
      </div>
      <div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={addRow}
          className="gap-1.5"
        >
          <PlusIcon data-icon="inline-start" />
          添加模型行
        </Button>
      </div>
    </div>
  );
}
