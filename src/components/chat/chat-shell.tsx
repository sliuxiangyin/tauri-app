import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
} from "@/components/ui/sidebar";
import Ai05 from "./ai-05";
import { useChatStore } from "@/stores/useChatStore";

type Session = {
  id: string;
  title: string;
  preview: string;
  time: string;
};

const sessions: Session[] = [
  { id: "1", title: "新会话", preview: "你好，今天有什么可以帮你?", time: "刚刚" },
  { id: "2", title: "项目讨论", preview: "我们继续聊聊架构…", time: "昨天" },
  { id: "3", title: "需求确认", preview: "这个功能的优先级…", time: "3 天前" },
];

function ChatSessionsSidebar({
  activeId,
  onSelect,
}: {
  activeId: string;
  onSelect: (id: string) => void;
}) {
  return (
    <Sidebar
        variant="inset"
      collapsible="icon"
      className="absolute h-full p-0 py-2  pr-0 "
      style={
        {
          // "borderRightColor": "#efeaea ",
          // "boxShadow": "4px 0 12px -4px rgb(0 0 0 / 0.01)",
        } as React.CSSProperties
      }
    >
      <SidebarHeader>
        <div className="flex items-center justify-between px-2 py-1">
          <span className="text-sm font-semibold">会话</span>
          <Button size="sm" variant="ghost">
            新建
          </Button>
        </div>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>最近</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {sessions.map((s) => (
                <SidebarMenuItem key={s.id}>
                  <SidebarMenuButton
                    isActive={s.id === activeId}
                    onClick={() => onSelect(s.id)}
                    className="h-auto flex-col items-start gap-0.5"
                  >
                    <div className="flex w-full items-center justify-between">
                      <span className="text-sm font-medium">{s.title}</span>
                      <span className="text-xs text-muted-foreground">
                        {s.time}
                      </span>
                    </div>
                    <span className="line-clamp-1 text-xs text-muted-foreground">
                      {s.preview}
                    </span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </Sidebar>
  );
}

export default function ChatShell() {
  const [open, setOpen] = useState(true);
  const [activeId, setActiveId] = useState("1");

  // 监听 store 中的消息加载状态
  const selectedAccount = useChatStore((s) => s.selectedAccount);
  const messages = useChatStore((s) => s.messages);
  const isLoading = useChatStore((s) => s.isLoading);
  useEffect(() => {
  }, [isLoading]);
  return (
    <SidebarProvider
      open={open}
      onOpenChange={setOpen}
      className="relative h-full min-h-0 w-full overflow-hidden "
      style={
        {
          "--sidebar-width": "calc(var(--spacing) * 52)",
          "--header-height": "calc(var(--spacing) * 12)",
        } as React.CSSProperties
      }
    >
      {/* 加载遮罩 */}
      {isLoading && (
        <div className="absolute left-0 right-0 inset-0 z-[9999] flex items-center justify-center bg-black/10 backdrop-blur-xs">
          <Spinner className="size-12 text-red-500" />
        </div>
      )}
      <ChatSessionsSidebar activeId={activeId} onSelect={setActiveId} />
      <SidebarInset className="flex min-h-0 flex-col relative">
       
        <Ai05
          account={selectedAccount}
          messages={messages}
        />
      </SidebarInset>
    </SidebarProvider>
  );
}
