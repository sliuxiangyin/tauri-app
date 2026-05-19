import * as React from "react";

import { SiderContent } from "@/components/common/sider-content";
import { McpProfileList } from "@/components/mcp-page/mcp-profile-list";
import { McpConfigEditor } from "@/components/mcp-page/mcp-config-editor";
import {
  McpServeConfigApi,
  type McpServeConfig,
} from "@/lib/mcp-serve-api";
import { useMcpStore } from "@/stores/useMcpStore";

interface McpModelConfig {
  transport: "stdio" | "http";
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
}

// ---------------------------------------------------------------------------
// JSON 转换工具
// ---------------------------------------------------------------------------

/** 用户输入格式 -> 内部格式：提取 name 和 config，去除 mcpServers 外层 */
function parseUserInput(json: Record<string, any>): { name: string; config: McpModelConfig } | null {
  const servers = json?.mcpServers;
  if (!servers || typeof servers !== "object") return null;
  const keys = Object.keys(servers);
  if (keys.length === 0) return null;
  const name = keys[0];
  const raw = servers[name];
  const transport: "stdio" | "http" = raw?.url ? "http" : "stdio";
  const config: McpModelConfig = {
    transport,
    command: raw?.command ?? undefined,
    args: raw?.args ?? undefined,
    env: raw?.env ?? undefined,
    url: raw?.url ?? undefined,
  };
  return { name, config };
}

/** 内部格式 -> 用户展示格式：加上 mcpServers 外层 */
function buildDisplayValue(cfg: McpServeConfig): Record<string, any> {
  const inner: Record<string, any> = {};
  if (cfg.config.command !== undefined) inner.command = cfg.config.command;
  if (cfg.config.args !== undefined) inner.args = cfg.config.args;
  if (cfg.config.env !== undefined) inner.env = cfg.config.env;
  if (cfg.config.url !== undefined) inner.url = cfg.config.url;
  inner.transport = cfg.config.transport;
  return {
    mcpServers: {
      [cfg.name]: inner,
    },
  };
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

let newIdCounter = 0;
function genTempId(): number {
  return --newIdCounter;
}

export default function McpPage() {
  // 使用 MCP Store 管理状态
  const configs = useMcpStore((state) => state.configs);
  const loading = useMcpStore((state) => state.loading);
  const error = useMcpStore((state) => state.error);
  const loadConfigs = useMcpStore((state) => state.loadConfigs);
  const createConfig = useMcpStore((state) => state.createConfig);
  const updateConfig = useMcpStore((state) => state.updateConfig);
  const deleteConfig = useMcpStore((state) => state.deleteConfig);
  const clearError = useMcpStore((state) => state.clearError);
  const initEventListener = useMcpStore((state) => state.initEventListener);

  const [selectedId, setSelectedId] = React.useState<number | null>(null);
  const [saving, setSaving] = React.useState(false);

  // 本地新建、尚未持久化的临时 ID
  const newIdsRef = React.useRef<Set<number>>(new Set());
  // 当前编辑器中的 JSON 值（含 mcpServers 外层）
  const [editorValue, setEditorValue] = React.useState<Record<string, any>>({});

  const selected = configs.find((c) => c.id === selectedId) ?? null;
  const isNew = selected ? newIdsRef.current.has(selected.id) : false;

  // ---- Fetch list & init event listener ----
  React.useEffect(() => {
    loadConfigs();
    initEventListener();
  }, [loadConfigs, initEventListener]);

  // 切换选中时同步编辑器内容
  React.useEffect(() => {
    if (selected) {
      setEditorValue(buildDisplayValue(selected));
    }
  }, [selectedId, selected]);
  // ---- Handlers ----
  // 本地临时配置列表（包含尚未保存的新配置）
  const [tempConfigs, setTempConfigs] = React.useState<McpServeConfig[]>([]);
  // 合并后端配置和本地临时配置
  const allConfigs = React.useMemo(() => {
    const tempIds = new Set(tempConfigs.map((c) => c.id));
    const persistedConfigs = configs.filter((c) => !tempIds.has(c.id));
    return [...persistedConfigs, ...tempConfigs];
  }, [configs, tempConfigs]);

  const addConfig = () => {
    const tempId = genTempId();
    const emptyConfig: McpServeConfig = {
      id: tempId,
      name: "",
      config: {
        transport: "stdio",
      },
      updated_at: new Date().toISOString(),
      state: false,
      tools: [],
      error: null,
      install_status: undefined,
    };
    setTempConfigs((prev) => [...prev, emptyConfig]);
    newIdsRef.current = new Set(newIdsRef.current).add(tempId);
    setSelectedId(tempId);
    setEditorValue({ mcpServers: { "": { transport: "stdio" } } });
    clearError();
  };

  const handleEditorChange = (value: Record<string, any>) => {
    setEditorValue(value);
    // 实时同步到本地临时配置状态
    const parsed = parseUserInput(value);
    if (parsed && selected) {
      setTempConfigs((prev) =>
        prev.map((c: McpServeConfig) =>
          c.id === selected.id ? { ...c, name: parsed.name, config: parsed.config } : c,
        ),
      );
    }
  };

  const handleSave = async () => {
    if (!selected || saving) return;
    const parsed = parseUserInput(editorValue);
    if (!parsed) {
      clearError();
      // 使用临时状态显示错误
      return;
    }
    if (!parsed.name) {
      clearError();
      return;
    }

    setSaving(true);
    try {
      if (isNew) {
        const created = await createConfig({
          name: parsed.name,
          config: parsed.config,
        });
        // 移除临时配置
        setTempConfigs((prev) => prev.filter((c: McpServeConfig) => c.id !== selected.id));
        setSelectedId(created.id);
        newIdsRef.current = new Set(
          [...newIdsRef.current].filter((id) => id !== selected.id),
        );
      } else {
        await updateConfig(selected.id, {
          name: parsed.name,
          config: parsed.config,
        });
      }
    } catch (e) {
      // 错误由 store 管理
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (id: number) => {
    // 乐观更新：先从临时配置移除
    setTempConfigs((prev) => prev.filter((c: McpServeConfig) => c.id !== id));
    if (selectedId === id) {
      setSelectedId(null);
      setEditorValue({});
    }

    if (newIdsRef.current.has(id)) {
      newIdsRef.current = new Set(
        [...newIdsRef.current].filter((x) => x !== id),
      );
      return;
    }

    try {
      await deleteConfig(id);
    } catch (e) {
      // 错误由 store 管理，刷新列表恢复
      loadConfigs();
    }
  };
  // ---- Render ----
  if (loading) {
    return (
      <div className="flex h-full w-full flex-col bg-white">
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-muted-foreground">加载中…</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full w-full flex-col bg-white">
      {error ? (
        <div className="mx-4 mt-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
          {error}
          <button
            type="button"
            className="ml-2 underline"
            onClick={() => clearError()}
          >
            关闭
          </button>
        </div>
      ) : null}
      <SiderContent
        className="min-h-0 flex-1"
        list={
          <McpProfileList
            configs={allConfigs}
            selectedId={selectedId}
            onSelect={(id) => setSelectedId(id)}
            onDelete={handleDelete}
            onAdd={addConfig}
          />
        }
        editor={
          selected ? (
            <div className="h-full min-h-0 p-4">
              <McpConfigEditor
                value={editorValue}
                isNew={isNew}
                saving={saving}
                onChange={handleEditorChange}
                onSave={handleSave}
                onDelete={() => handleDelete(selected.id)}
              />
            </div>
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-12 text-center">
              <p className="text-sm text-muted-foreground">
                请从左侧选择一条配置，或点击「添加」新建。
              </p>
            </div>
          )
        }
      />
    </div>
  );
}
