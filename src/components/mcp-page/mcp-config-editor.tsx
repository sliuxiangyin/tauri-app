import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { JsonEditor } from "./json-editor";

type McpConfigEditorProps = {
  value: Record<string, any>;
  isNew: boolean;
  saving: boolean;
  onChange: (value: Record<string, any>) => void;
  onSave: () => void;
  onDelete: () => void;
};

export function McpConfigEditor({
  value,
  isNew,
  saving,
  onChange,
  onSave,
  onDelete,
}: McpConfigEditorProps) {
  return (
    <Card className="border-0 shadow-none ring-0">
      <CardHeader className="border-b border-border">
        <div className="flex flex-row items-center justify-between gap-1">
          <CardTitle>编辑配置</CardTitle>
          <CardAction className="flex items-center gap-2">
            <Button
              type="button"
              variant="destructive"
              size="sm"
              onClick={onDelete}
            >
              删除
            </Button>
            <Button
              type="button"
              variant="default"
              size="sm"
              disabled={saving}
              onClick={onSave}
            >
              {saving ? "保存中…" : isNew ? "创建" : "保存"}
            </Button>
          </CardAction>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 pt-4">
        <JsonEditor value={value} onChange={onChange} />
      </CardContent>
    </Card>
  );
}
