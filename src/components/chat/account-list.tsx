import React, { useEffect, useState } from "react";
import { Plus } from "lucide-react";
import { cn } from "@/lib/utils";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";

import { Separator } from "@/components/ui/separator";
import {
    Sidebar,
    SidebarContent,
    SidebarHeader,
    SidebarFooter,
    SidebarTrigger,
    useSidebar,
} from "@/components/ui/sidebar";
import { getAccounts, AccountInfo } from "@/lib/wechat-api";
import AccountItem from "./account-item";
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
const AccountListContent = () => {
    const [accounts, setAccounts] = useState<AccountInfo[]>([]);
    const [activeId, setActiveId] = useState("");
    const { state } = useSidebar();
    const isCollapsed = state === "collapsed";
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    useEffect(() => {
        loadAccounts();
    }, []);
    // 加载账户列表
    const loadAccounts = async () => {
        try {
            const response = await getAccounts();
            setAccounts(response.accounts);
        } catch (error) {
            console.error("加载账户列表失败:", error);
        }
    };
    // 添加账户的处理函数
    const handleAddAccount = () => {
        setIsDialogOpen(true);
    };

    // 登录成功后的回调
    const handleLoginSuccess = async () => {
        // 刷新账户列表
        await loadAccounts();
    };

    return (
        <div className="flex h-full flex-col">
            <SidebarHeader className="relative shrink-0">
                <div className={cn(
                    "flex items-center p-4 transition-all",
                    isCollapsed
                        ? "justify-center p-2"   // 收缩时 p-2
                        : "justify-between p-4"  // 展开时 p-4
                )}>
                    {!isCollapsed && (
                        <div>
                            <h2 className="text-lg font-semibold tracking-tight">微信bot</h2>
                            <p className="text-xs text-muted-foreground mt-1">
                                {accounts.length} account{accounts.length !== 1 ? "s" : ""}
                            </p>
                        </div>
                    )}
                    <SidebarTrigger
                        className={cn(
                            "h-7 w-7",
                            isCollapsed && "rotate-180"
                        )}
                    />
                </div>
                {!isCollapsed && <Separator />}
            </SidebarHeader>

            <SidebarContent className={cn("flex-1 px-2 min-h-0", isCollapsed && "px-1")}>
                <ScrollArea className="h-full">
                    <div className="space-y-0.5 py-2">
                        {accounts.map((account) => (
                            <AccountItem
                                key={account.accountId}
                                account={account}
                                isActive={activeId === account.accountId}
                                isCollapsed={isCollapsed}
                                onClick={setActiveId}
                                onDelete={(accountId) => {
                                    console.log("删除账户:", accountId);
                                    // TODO: 调用删除账户 API
                                }}
                            />
                        ))}
                    </div>
                </ScrollArea>
            </SidebarContent>

            <SidebarFooter className="shrink-0 p-3">
                <Tooltip>
                    <TooltipTrigger asChild>
                        <Button
                            variant="outline"
                            size="sm"
                            className={cn(
                                "rounded-xl border-dashed hover:border-solid hover:bg-accent hover:text-accent-foreground transition-all group",
                                isCollapsed ? "w-10 h-10 p-0 mx-auto" : "w-full"
                            )}
                            onClick={handleAddAccount}
                        >
                            <Plus className={cn(
                                "h-4 w-4 transition-transform group-hover:scale-110",
                                !isCollapsed && "mr-2"
                            )} />
                            {!isCollapsed && <span className="text-sm">添加账户</span>}
                        </Button>
                    </TooltipTrigger>
                    {isCollapsed && (
                        <TooltipContent side="right">
                            <p>添加账户</p>
                        </TooltipContent>
                    )}
                </Tooltip>
            </SidebarFooter>

            {/* 二维码登录对话框组件 */}
            {/*<QRCodeLoginDialog*/}
            {/*    open={isDialogOpen}*/}
            {/*    onOpenChange={setIsDialogOpen}*/}
            {/*    onLoginSuccess={handleLoginSuccess}*/}
            {/*/>*/}
        </div>
    );
};

const AppSidebar = () => {
    return (
        <Sidebar
            collapsible="icon"
            variant="sidebar"
            className="relative h-full border-r"
            style={
                {
                    "--sidebar-width-icon": "4rem", // 4rem = 64px
                } as React.CSSProperties
            }
        >
            <AccountListContent />
        </Sidebar>
    );
};

export default AppSidebar;
