import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

import { ModelEntriesEditor } from "./model-entries-editor";
import type { ModelConfigProfile, ProviderKind } from "./types";

const providerOptions: { value: ProviderKind; label: string }[] = [
  { value: "open_ai", label: "OpenAI 兼容" },
  { value: "ollama", label: "Ollama" },
];

type ConfigEditorFormProps = {
  profile: ModelConfigProfile;
  isNew: boolean;
  saving: boolean;
  onChange: (next: ModelConfigProfile) => void;
  onSave: () => void;
  onDelete: () => void;
};

export function ConfigEditorForm({
  profile,
  isNew,
  saving,
  onChange,
  onSave,
}: ConfigEditorFormProps) {
  const patch = (partial: Partial<ModelConfigProfile>) => {
    onChange({ ...profile, ...partial });
  };

  const displayNameInvalid = profile.displayName.trim().length === 0;
  const apiBaseUrlInvalid = profile.apiBaseUrl.trim().length === 0;

  return (
    <Card className="border-0 shadow-none ring-0">
      <CardHeader className="border-b border-border">
        <div className="flex  flex-row items-center justify-between gap-1">
          <CardTitle>编辑配置</CardTitle>
          <CardAction>
          <Button
            type="button"
            variant="default"
            disabled={saving}
            onClick={onSave}
          >
            {saving ? "保存中…" : isNew ? "创建" : "保存"}
          </Button>
         
        </CardAction>
        </div>
       
      </CardHeader>
      <CardContent className="flex flex-col gap-4 pt-4">
        <div className="flex flex-col gap-2">
          <Label
            htmlFor={`cfg-name-${profile.id}`}
            className="inline-flex items-center gap-0.5"
          >
            显示名称
            <span className="text-destructive" aria-hidden>
              *
            </span>
          </Label>
          <Input
            id={`cfg-name-${profile.id}`}
            value={profile.displayName}
            onChange={(e) => patch({ displayName: e.target.value })}
            placeholder="列表中显示的名称"
            required
            aria-invalid={displayNameInvalid}
            aria-label="显示名称（必填）"
          />
        </div>

        <div className="flex flex-row items-center justify-between gap-4 rounded-lg border border-border px-3 py-2">
          <Label htmlFor={`cfg-enabled-${profile.id}`} className="font-medium">
            启用
          </Label>
          <Switch
            id={`cfg-enabled-${profile.id}`}
            checked={profile.enabled}
            onCheckedChange={(v) => patch({ enabled: Boolean(v) })}
          />
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor={`cfg-provider-${profile.id}`}>提供商类型</Label>
          <Select
            value={profile.provider}
            onValueChange={(v) => {
              const provider = v as ProviderKind;
              patch({
                provider,
                apiKey: provider === "ollama" ? "" : profile.apiKey,
              });
            }}
          >
            <SelectTrigger
              id={`cfg-provider-${profile.id}`}
              className="w-full min-w-0"
              size="default"
            >
              <SelectValue placeholder="选择提供商" />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectGroup>
                {providerOptions.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-2">
          <Label
            htmlFor={`cfg-url-${profile.id}`}
            className="inline-flex items-center gap-0.5"
          >
            API 地址
            <span className="text-destructive" aria-hidden>
              *
            </span>
          </Label>
          <Input
            id={`cfg-url-${profile.id}`}
            value={profile.apiBaseUrl}
            onChange={(e) => patch({ apiBaseUrl: e.target.value })}
            placeholder="https://…"
            autoComplete="off"
            required
            aria-invalid={apiBaseUrlInvalid}
            aria-label="API 地址（必填）"
          />
        </div>

        {profile.provider === "open_ai" ? (
          <div className="flex flex-col gap-2">
            <Label htmlFor={`cfg-key-${profile.id}`}>API 密钥</Label>
            <Input
              id={`cfg-key-${profile.id}`}
              type="password"
              value={profile.apiKey}
              onChange={(e) => patch({ apiKey: e.target.value })}
              placeholder="sk-…"
              autoComplete="off"
            />
          </div>
        ) : null}

        <ModelEntriesEditor
          models={profile.models}
          onChange={(models) => patch({ models })}
        />
      </CardContent>
    </Card>
  );
}
