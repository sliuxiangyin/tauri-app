import { useCallback, useEffect, useMemo, useState } from "react";
import { IconX } from "@tabler/icons-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { getRunningMcps } from "@/lib/api/mcp";
import type { McpDto } from "@/lib/api/mcp";
import { deleteChatToolsConfig, getChatToolsConfig, saveChatToolsConfig } from "@/lib/api/chat_tools";
import type { ChatToolsConfig } from "@/lib/api/chat_tools";

export type McpToolItem = {
  id: string;
  name: string;
  description?: string;
};

export type McpServerItem = {
  id: string;
  name: string;
  tools: McpToolItem[];
};

type Ai05ToolPanelProps = {
  sessionId: string;
  accountId: string;
  show: boolean;
  onClose: () => void;
};

export function Ai05ToolPanel(props: Ai05ToolPanelProps) {
  const { sessionId, accountId, show, onClose } = props;
  const [mcpServers, setMcpServers] = useState<McpServerItem[]>([]);
  const [selectedServerId, setSelectedServerId] = useState<string>("");
  const [serverSearch, setServerSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [enabledServerIds, setEnabledServerIds] = useState<string[]>([]);
  const [selectedToolsByServer, setSelectedToolsByServer] = useState<Record<string, string[]>>({});

  const filteredServers = useMemo(() => {
    const keyword = serverSearch.trim().toLowerCase();
    if (!keyword) return mcpServers;
    return mcpServers.filter((server) => server.name.toLowerCase().includes(keyword));
  }, [mcpServers, serverSearch]);

  const selectedServer = useMemo(
    () =>
      filteredServers.find((server) => server.id === selectedServerId) ??
      mcpServers.find((server) => server.id === selectedServerId) ??
      filteredServers[0] ??
      mcpServers[0],
    [filteredServers, mcpServers, selectedServerId]
  );

  const selectedCountByServer = useMemo(
    () =>
      Object.fromEntries(
        mcpServers.map((server) => [server.id, selectedToolsByServer[server.id]?.length ?? 0])
      ),
    [mcpServers, selectedToolsByServer]
  );

  const handleToggleTool = useCallback((serverId: string, toolId: string, checked: boolean) => {
    setSelectedToolsByServer((prev) => {
      const current = new Set(prev[serverId] ?? []);
      if (checked) current.add(toolId);
      else current.delete(toolId);
      return { ...prev, [serverId]: [...current] };
    });
  }, []);

  const handleSelectAllTools = useCallback((serverId: string) => {
    const target = mcpServers.find((server) => server.id === serverId);
    if (!target) return;
    setSelectedToolsByServer((prev) => ({
      ...prev,
      [serverId]: target.tools.map((tool) => tool.id)
    }));
  }, [mcpServers]);

  const handleClearTools = useCallback((serverId: string) => {
    setSelectedToolsByServer((prev) => ({ ...prev, [serverId]: [] }));
  }, []);

  const handleToggleServer = useCallback((serverId: string, checked: boolean) => {
    setEnabledServerIds((prev) => {
      const next = new Set(prev);
      if (checked) next.add(serverId);
      else next.delete(serverId);
      return [...next];
    });
  }, []);

  useEffect(() => {
    // 仅在面板显示时加载数据
    if (!show) return;

    let cancelled = false;
    const loadToolPanelState = async () => {
      setLoading(true);
      try {
        const mcps = await getRunningMcps();
        console.log("mcps:", mcps);
        if (cancelled) return;
        // 解析每个 MCP 的 tools 字段
        const nextServers: McpServerItem[] = mcps
          .filter((mcp): mcp is McpDto & { tools: string } => !!mcp.tools)
          .map((mcp) => {
            let tools: McpToolItem[] = [];
            try {
              const parsed = JSON.parse(mcp.tools);
              if (Array.isArray(parsed)) {
                tools = parsed.map((t: { name: string; description?: string }) => ({
                  id: t.name,
                  name: t.name,
                  description: t.description
                }));
              }
            } catch {
              // 解析失败，tools 为空数组
            }
            return {
              id: mcp.name,
              name: mcp.name,
              tools
            };
          });

        // 获取已保存的工具配置
        let savedConfig: ChatToolsConfig = { disabled_servers: [], disabled_tools: {} };
        try {
          savedConfig = await getChatToolsConfig(accountId, sessionId);
        } catch (e) {
          // 无保存的配置，使用默认
          console.log("无已保存的工具配置，使用默认");
        }
        if (cancelled) return;

        // 根据保存的配置设置状态
        // disabled_servers 中的为禁用，其余启用
        const enabledServers = nextServers
          .map((s) => s.id)
          .filter((id) => !savedConfig.disabled_servers.includes(id));

        // 对于启用的 server，从 disabled_tools 中获取被禁用的工具
        const selectedTools: Record<string, string[]> = {};
        for (const server of nextServers) {
          if (enabledServers.includes(server.id)) {
            const disabledToolIds = savedConfig.disabled_tools[server.id] ?? [];
            selectedTools[server.id] = server.tools
              .map((t) => t.id)
              .filter((id) => !disabledToolIds.includes(id));
          } else {
            // 禁用的 server 不选中任何工具
            selectedTools[server.id] = [];
          }
        }

        setMcpServers(nextServers);
        setEnabledServerIds(enabledServers);
        setSelectedToolsByServer(selectedTools);
        setSelectedServerId("");
      } catch (e) {
        if (cancelled) return;
        window.alert(e instanceof Error ? e.message : "加载工具面板失败");
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void loadToolPanelState();
    return () => {
      cancelled = true;
    };
  }, [show, accountId, sessionId]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      // 构建禁用配置
      const disabledServers = mcpServers
        .map((s) => s.id)
        .filter((serverId) => !enabledServerIds.includes(serverId));
      const disabledTools: Record<string, string[]> = {};
      for (const server of mcpServers) {
        if (!enabledServerIds.includes(server.id)) continue;
        const selected = new Set(selectedToolsByServer[server.id] ?? []);
        const forbiddenTools = server.tools.map((t) => t.id).filter((id) => !selected.has(id));
        if (forbiddenTools.length > 0) {
          disabledTools[server.id] = forbiddenTools;
        }
      }
      const config: ChatToolsConfig = { disabled_servers: disabledServers, disabled_tools: disabledTools };
      console.log("保存工具配置:", config);
      await saveChatToolsConfig(accountId, sessionId, config);
    } catch (e) {
      window.alert(e instanceof Error ? e.message : "保存工具面板失败");
    } finally {
      setSaving(false);
    }
  }, [accountId, sessionId, enabledServerIds, mcpServers, selectedToolsByServer]);

  return (
    <div
      className={[
        "absolute bottom-full left-3 right-3 z-20 mb-2 overflow-hidden rounded-md border border-border/80 bg-card shadow-lg transition-all duration-200 ease-out",
        show
          ? "max-h-[360px] translate-y-0 opacity-100"
          : "max-h-0 translate-y-1 border-transparent opacity-0 pointer-events-none"
      ].join(" ")}
      aria-hidden={!show}
    >
      
        <div className="flex h-[320px] min-w-0">
          <div className="w-[230px] border-r border-border/70 p-2">
            <p className="px-1 py-1 text-xs font-medium text-muted-foreground">MCP Server</p>
            <Input
              value={serverSearch}
              onChange={(e) => setServerSearch(e.target.value)}
              placeholder="搜索 server..."
              className="h-8"
            />
            <div className="mt-1.5 space-y-0.5">
              {loading ? (
                <p className="px-2 py-3 text-xs text-muted-foreground">加载中...</p>
              ) : null}
              {filteredServers.map((server) => {
                console.log("server:", server);
                const active = server.id === selectedServer?.id;
                const enabled = enabledServerIds.includes(server.id);
                const selectedCount = selectedCountByServer[server.id] ?? 0;
                return (
                  <div
                    key={server.id}
                    className={[
                      "flex items-start gap-2 rounded-md px-2 py-0.5 transition-colors",
                      active
                        ? "bg-accent text-accent-foreground"
                        : "text-muted-foreground hover:bg-muted hover:text-foreground"
                    ].join(" ")}
                  >
                    <Checkbox
                      checked={enabled}
                      onCheckedChange={(value) => handleToggleServer(server.id, value === true)}
                      className="mt-0.5"
                    />
                    <button
                      type="button"
                      onClick={() => setSelectedServerId(server.id)}
                      className="min-w-0 flex-1 text-left"
                    >
                      <div className="truncate font-medium">{server.name}</div>
                      <div className="mt-0.5 text-[10px]">
                        {enabled ? "启用" : "禁用"} · 已选 {selectedCount}/{server.tools.length}
                      </div>
                    </button>
                  </div>
                );
              })}
              {filteredServers.length === 0 ? (
                <p className="px-2 py-3 text-xs text-muted-foreground">未找到匹配的 MCP Server</p>
              ) : null}
            </div>
          </div>
          <div className="flex min-w-0 flex-1 flex-col p-3">
            <div className="mb-1.5 flex items-center justify-between">
              <div>
                <p className="text-xs font-medium">{selectedServer?.name ?? "MCP Server"}</p>
                <p className="text-xs text-muted-foreground">选择此服务允许使用的工具</p>
              </div>
              <div className="flex items-center gap-1 mr-5">
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-7 px-2 text-xs"
                  disabled={!selectedServer || !enabledServerIds.includes(selectedServer.id)}
                  onClick={() => selectedServer && handleSelectAllTools(selectedServer.id)}
                >
                  全选
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-7 px-2 text-xs"
                  disabled={!selectedServer || !enabledServerIds.includes(selectedServer.id)}
                  onClick={() => selectedServer && handleClearTools(selectedServer.id)}
                >
                  清空
                </Button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className=" right-0 top-2 z-10 size-7 text-muted-foreground hover:text-foreground"
                  aria-label="关闭工具面板"
                  title="关闭工具面板"
                  onClick={onClose}
                >
                  <IconX className="size-4" />
                </Button>
              </div>
            </div>
            <div
              className={[
                "min-h-0 min-w-0 flex-1 space-y-1 overflow-y-auto rounded-md border border-border/70 p-1.5",
                selectedServer && !enabledServerIds.includes(selectedServer.id) ? "opacity-50" : ""
              ].join(" ")}
            >
              {selectedServer?.tools.length ? (
                selectedServer.tools.map((tool) => {
                  const checked = (selectedToolsByServer[selectedServer.id] ?? []).includes(tool.id);
                  return (
                    <label
                      key={tool.id}
                      className="flex w-full cursor-pointer items-start gap-2 overflow-hidden rounded-md px-2 py-0.5 hover:bg-muted/60"
                    >
                      <Checkbox
                        checked={checked}
                        onCheckedChange={(value) => handleToggleTool(selectedServer.id, tool.id, value === true)}
                        disabled={!enabledServerIds.includes(selectedServer.id)}
                        className="mt-0.5"
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-xs font-medium">{tool.name}</span>
                        <span
                          className="block max-w-full truncate text-xs text-muted-foreground"
                          title={tool.description ?? "无描述"}
                        >
                          {tool.description ?? "无描述"}
                        </span>
                      </span>
                    </label>
                  );
                })
              ) : (
                <p className="px-2 py-4 text-xs text-muted-foreground">该服务暂无可用工具</p>
              )}
            </div>
            <div className="mt-2 flex items-center gap-2">
              <p className="min-w-0 flex-1 text-xs text-muted-foreground">
                服务{selectedServer && enabledServerIds.includes(selectedServer.id) ? "已启用" : "已禁用"} ·
                已选 {selectedCountByServer[selectedServer?.id ?? ""] ?? 0}/{selectedServer?.tools.length ?? 0}
              </p>
              <div className="flex shrink-0 items-center gap-1">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  className="h-7 px-2 text-xs"
                  disabled={saving || loading}
                  onClick={async () => {
                    // 调用 API 删除保存的配置
                    try {
                      await deleteChatToolsConfig(accountId, sessionId);
                    } catch (e) {
                      console.log("恢复默认配置失败:", e);
                    }
                    // 前端状态恢复默认：全部启用
                    setEnabledServerIds(mcpServers.map((server) => server.id));
                    setSelectedToolsByServer(
                      Object.fromEntries(mcpServers.map((server) => [server.id, server.tools.map((tool) => tool.id)]))
                    );
                  }}
                >
                  恢复默认（全部）
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="default"
                  className="h-7 px-2 text-xs"
                  disabled={saving || loading}
                  onClick={() => void handleSave()}
                >
                  {saving ? "保存中..." : "应用"}
                </Button>
              </div>
            </div>
          </div>
        </div>
      
    </div>
  );
}
