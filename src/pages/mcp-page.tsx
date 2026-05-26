import { useState, useEffect } from 'react';
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Drawer,
  DrawerContent,
  DrawerHeader,
  DrawerTitle,
  DrawerFooter,
  DrawerClose,
} from "@/components/ui/drawer";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { X } from 'lucide-react';
import { 
  ChevronDown, 
  ChevronRight, 
  Link2, 
  Bell, 
  RefreshCw, 
  Pencil, 
  Trash2, 
  Settings, 
  Plus,
  Loader2
} from 'lucide-react';
import { 
  getAllMcps, 
  createMcp, 
  updateMcp, 
  deleteMcp, 
  toggleMcpStatus,
  McpDto 
} from '@/lib/api/mcp';

export default function McpPage() {
  const [mcps, setMcps] = useState<McpDto[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isAddDrawerOpen, setIsAddDrawerOpen] = useState(false);
  const [isEditDrawerOpen, setIsEditDrawerOpen] = useState(false);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [currentMcp, setCurrentMcp] = useState<McpDto | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  
  // 表单状态
  const [formName, setFormName] = useState('');
  const [configJson, setConfigJson] = useState('');
  const [formStatus, setFormStatus] = useState('enable');
  const [expandedCards, setExpandedCards] = useState<Set<number>>(new Set());

  // 加载 MCP 列表
  const loadMcps = async () => {
    setIsLoading(true);
    try {
      const data = await getAllMcps();
      console.log('Loaded MCPs:', data); 
      setMcps(data);
    } catch (error) {
      console.error('加载 MCP 列表失败:', error);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadMcps();
  }, []);

  // 打开添加 Drawer
  const handleOpenAddDrawer = () => {
    setFormName('');
    setConfigJson('');
    setFormStatus('enable');
    setIsAddDrawerOpen(true);
  };

  // 打开编辑 Drawer
  const handleOpenEditDrawer = (mcp: McpDto) => {
    setCurrentMcp(mcp);
    setFormName(mcp.name);
    setConfigJson(mcp.config);
    setFormStatus(mcp.status);
    setIsEditDrawerOpen(true);
  };

  // 提交添加
  const handleSubmitAdd = async () => {
    if (!formName.trim()) {
      alert('请输入服务名称');
      return;
    }
    if (!configJson.trim()) {
      alert('请输入 MCP 配置 JSON');
      return;
    }

    setIsSubmitting(true);
    try {
      await createMcp(formName.trim(), configJson.trim(), formStatus);
      await loadMcps();
      setIsAddDrawerOpen(false);
    } catch (error) {
      console.error('创建 MCP 失败:', error);
      alert(`创建失败: ${error}`);
    } finally {
      setIsSubmitting(false);
    }
  };

  // 提交编辑
  const handleSubmitEdit = async () => {
    if (!currentMcp) return;
    
    setIsSubmitting(true);
    try {
      await updateMcp(currentMcp.name, configJson.trim(), formStatus);
      await loadMcps();
      setIsEditDrawerOpen(false);
      setCurrentMcp(null);
    } catch (error) {
      console.error('更新 MCP 失败:', error);
      alert(`更新失败: ${error}`);
    } finally {
      setIsSubmitting(false);
    }
  };

  // 打开删除确认
  const handleOpenDelete = (mcp: McpDto) => {
    setCurrentMcp(mcp);
    setIsDeleteDialogOpen(true);
  };

  // 确认删除
  const handleConfirmDelete = async () => {
    if (!currentMcp) return;
    
    setIsSubmitting(true);
    try {
      await deleteMcp(currentMcp.name);
      await loadMcps();
      setIsDeleteDialogOpen(false);
      setCurrentMcp(null);
    } catch (error) {
      console.error('删除 MCP 失败:', error);
      alert(`删除失败: ${error}`);
    } finally {
      setIsSubmitting(false);
    }
  };

  // 切换状态
  const handleToggleStatus = async (mcp: McpDto) => {
    try {
      await toggleMcpStatus(mcp.name);
      await loadMcps();
    } catch (error) {
      console.error('切换状态失败:', error);
    }
  };

  // 切换卡片展开
  const toggleCard = (id: number) => {
    const newExpanded = new Set(expandedCards);
    if (newExpanded.has(id)) {
      newExpanded.delete(id);
    } else {
      newExpanded.add(id);
    }
    setExpandedCards(newExpanded);
  };

  // 解析工具列表
  const parseTools = (tools?: string) => {
    if (!tools) return [];
    try {
      return JSON.parse(tools);
    } catch {
      return [];
    }
  };

  // 解析 config 中的 url
  const parseUrl = (config: string) => {
    try {
      const obj = JSON.parse(config);
      return obj.url || obj.command || '';
    } catch {
      return '';
    }
  };

  return (
    <div className="min-h-screen w-full bg-background text-foreground p-8 font-sans">
      
      {/* 头部标题区 */}
      <div className="flex justify-between items-start mb-8">
        <div>
          <h1 className="text-xl font-semibold mb-2 text-foreground">MCP 服务</h1>
          <p className="text-muted-foreground text-sm">
            安装新的 MCP 服务为智能体扩展更多工具。如需了解更多，可查看{' '}
            <a href="#" className="text-primary hover:text-primary/80 cursor-pointer">文档</a>
          </p>
        </div>
        <Button variant="secondary" className="h-8 px-3 rounded-md" onClick={handleOpenAddDrawer}>
          <Plus className="w-4 h-4 mr-1.5" /> 添加
        </Button>
      </div>

      {/* 自定义 Tabs 导航 */}
      <div className="flex gap-6 border-b border-border mb-6 text-sm">
        <div className="pb-3 border-b-2 border-primary text-primary font-medium cursor-pointer">
          我的服务
        </div>
      </div>

      {/* 加载状态 */}
      {isLoading ? (
        <div className="flex items-center justify-center py-12">
          <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
          <span className="ml-2 text-muted-foreground">加载中...</span>
        </div>
      ) : mcps.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          暂无 MCP 服务，点击"添加"创建一个
        </div>
      ) : (
        /* MCP 服务卡片列表 */
        <div className="space-y-4">
          {mcps.map((mcp) => {
            const tools = parseTools(mcp.tools);
            const urlOrCommand = parseUrl(mcp.config);
            const isExpanded = expandedCards.has(mcp.id);
            
            return (
              <Collapsible 
                key={mcp.id}
                open={isExpanded}
                onOpenChange={() => toggleCard(mcp.id)}
                className="bg-card border border-border rounded-xl overflow-hidden shadow-sm"
              >
                {/* 卡片 Header */}
                <div className="flex items-center justify-between p-3.5 px-4 bg-card">
                  <div className="flex items-center gap-3">
                    <CollapsibleTrigger asChild>
                      <div className="cursor-pointer text-muted-foreground hover:text-foreground transition-colors">
                        {isExpanded ? <ChevronDown className="w-[18px] h-[18px]" /> : <ChevronRight className="w-[18px] h-[18px]" />}
                      </div>
                    </CollapsibleTrigger>
                    <Link2 className="w-[18px] h-[18px] text-primary" />
                    <span className="text-sm font-medium text-foreground">{mcp.name}</span>
                    <span className={`text-xs px-2 py-0.5 rounded ${
                      mcp.status === 'enable' 
                        ? 'bg-green-100 text-green-700' 
                        : 'bg-gray-100 text-gray-500'
                    }`}>
                      {mcp.status === 'enable' ? '已启用' : '已禁用'}
                    </span>
                  </div>
                  
                  {/* 右侧操作区 */}
                  <div className="flex items-center gap-4">
                    <div className="flex items-center gap-3.5 mr-2">
                      <Bell className="w-4 h-4 text-muted-foreground cursor-pointer hover:text-foreground transition-colors" />
                      <RefreshCw className="w-4 h-4 text-muted-foreground cursor-pointer hover:text-foreground transition-colors" onClick={() => loadMcps()} />
                      <Pencil className="w-4 h-4 text-muted-foreground cursor-pointer hover:text-foreground transition-colors" onClick={() => handleOpenEditDrawer(mcp)} />
                      <Trash2 className="w-4 h-4 text-muted-foreground cursor-pointer hover:text-foreground transition-colors" onClick={() => handleOpenDelete(mcp)} />
                    </div>
                    <Switch 
                      checked={mcp.status === 'enable'}
                      onCheckedChange={() => handleToggleStatus(mcp)}
                    />
                  </div>
                </div>

                {/* 卡片展开内容 */}
                <CollapsibleContent className="px-5 pb-6 pt-2">
                  {/* URL/Command 区域 */}
                  <div className="mb-6">
                    <div className="text-muted-foreground text-sm mb-1.5">配置</div>
                    <div className="text-foreground text-[13px] tracking-wide font-mono bg-muted p-2 rounded">
                      {urlOrCommand}
                    </div>
                  </div>

                  {/* 工具列表区域 */}
                  <div className="mb-8">
                    <div className="text-muted-foreground text-sm mb-3">工具({tools.length})</div>
                    {tools.length > 0 ? (
                      <div className="flex flex-col gap-2.5">
                        {tools.map((tool: string, index: number) => (
                          <div key={index} className="grid grid-cols-[1fr_2fr] gap-4 text-[13px] items-center">
                            <div className="flex items-center gap-2.5 text-foreground">
                              <Settings className="w-4 h-4 text-muted-foreground" />
                              <span>{tool}</span>
                            </div>
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="text-muted-foreground text-sm">暂无工具</div>
                    )}
                  </div>
                </CollapsibleContent>
              </Collapsible>
            );
          })}
        </div>
      )}

      {/* 添加 MCP 服务 Drawer */}
      <Drawer open={isAddDrawerOpen} onOpenChange={setIsAddDrawerOpen} direction="right">
        <DrawerContent className="w-[500px] max-w-full">
          <DrawerHeader className="border-b">
            <div className="flex items-center justify-between">
              <DrawerTitle>添加 MCP 服务</DrawerTitle>
              <DrawerClose asChild>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  <X className="h-4 w-4" />
                </Button>
              </DrawerClose>
            </div>
          </DrawerHeader>
          <div className="p-4 flex-1 space-y-4">
            {/* 名称 */}
            <div className="space-y-2">
              <Label htmlFor="name">服务名称</Label>
              <Input
                id="name"
                placeholder="输入服务名称"
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
              />
            </div>
            {/* 状态 */}
            <div className="space-y-2">
              <Label htmlFor="status">状态</Label>
              <Select value={formStatus} onValueChange={setFormStatus}>
                <SelectTrigger id="status">
                  <SelectValue placeholder="选择状态" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="enable">启用</SelectItem>
                  <SelectItem value="disable">禁用</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {/* 配置 JSON */}
            <div className="space-y-2">
              <Label htmlFor="config">配置 (JSON)</Label>
              <Textarea
                id="config"
                placeholder={`输入 MCP 服务配置 JSON...\n\n例如 STDIO:\n{\n  "command": "/path/to/mcp-proxy",\n  "args": ["--transport", "stdio"]\n}\n\n例如 HTTP:\n{\n  "url": "https://example.com/mcp"`}
                value={configJson}
                onChange={(e) => setConfigJson(e.target.value)}
                className="min-h-[250px] font-mono text-xs"
              />
            </div>
          </div>
          <DrawerFooter className="border-t flex-row justify-end gap-2">
            <DrawerClose asChild>
              <Button variant="outline">取消</Button>
            </DrawerClose>
            <Button 
              variant="default" 
              onClick={handleSubmitAdd}
              disabled={isSubmitting}
            >
              {isSubmitting ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : null}
              确认添加
            </Button>
          </DrawerFooter>
        </DrawerContent>
      </Drawer>

      {/* 编辑 MCP 服务 Drawer */}
      <Drawer open={isEditDrawerOpen} onOpenChange={setIsEditDrawerOpen} direction="right">
        <DrawerContent className="w-[500px] max-w-full">
          <DrawerHeader className="border-b">
            <div className="flex items-center justify-between">
              <DrawerTitle>编辑 MCP 服务</DrawerTitle>
              <DrawerClose asChild>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  <X className="h-4 w-4" />
                </Button>
              </DrawerClose>
            </div>
          </DrawerHeader>
          <div className="p-4 flex-1 space-y-4">
            {/* 名称 (只读) */}
            <div className="space-y-2">
              <Label htmlFor="edit-name">服务名称</Label>
              <Input
                id="edit-name"
                value={formName}
                disabled
                className="bg-muted"
              />
            </div>
            {/* 状态 */}
            <div className="space-y-2">
              <Label htmlFor="edit-status">状态</Label>
              <Select value={formStatus} onValueChange={setFormStatus}>
                <SelectTrigger id="edit-status">
                  <SelectValue placeholder="选择状态" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="enable">启用</SelectItem>
                  <SelectItem value="disable">禁用</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {/* 配置 JSON */}
            <div className="space-y-2">
              <Label htmlFor="edit-config">配置 (JSON)</Label>
              <Textarea
                id="edit-config"
                placeholder="输入 MCP 服务配置 JSON..."
                value={configJson}
                onChange={(e) => setConfigJson(e.target.value)}
                className="min-h-[250px] font-mono text-xs"
              />
            </div>
          </div>
          <DrawerFooter className="border-t flex-row justify-end gap-2">
            <DrawerClose asChild>
              <Button variant="outline">取消</Button>
            </DrawerClose>
            <Button 
              variant="default" 
              onClick={handleSubmitEdit}
              disabled={isSubmitting}
            >
              {isSubmitting ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : null}
              保存修改
            </Button>
          </DrawerFooter>
        </DrawerContent>
      </Drawer>

      {/* 删除确认 Dialog */}
      <Drawer open={isDeleteDialogOpen} onOpenChange={setIsDeleteDialogOpen} direction="right">
        <DrawerContent className="w-[400px] max-w-full">
          <DrawerHeader className="border-b">
            <div className="flex items-center justify-between">
              <DrawerTitle>确认删除</DrawerTitle>
              <DrawerClose asChild>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  <X className="h-4 w-4" />
                </Button>
              </DrawerClose>
            </div>
          </DrawerHeader>
          <div className="p-4">
            <p className="text-muted-foreground">
              确定要删除 MCP 服务 <span className="font-medium text-foreground">{currentMcp?.name}</span> 吗？此操作无法撤销。
            </p>
          </div>
          <DrawerFooter className="border-t flex-row justify-end gap-2">
            <DrawerClose asChild>
              <Button variant="outline">取消</Button>
            </DrawerClose>
            <Button 
              variant="destructive" 
              onClick={handleConfirmDelete}
              disabled={isSubmitting}
            >
              {isSubmitting ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : null}
              删除
            </Button>
          </DrawerFooter>
        </DrawerContent>
      </Drawer>

    </div>
  );
}