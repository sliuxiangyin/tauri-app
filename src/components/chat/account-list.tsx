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
import { useChatStore } from "@/stores/useChatStore";
import AccountItem from "./account-item";
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { QRCodeLoginDialog } from "./qr-code-login-dialog";

const AccountListContent = () => {
    const [accounts, setAccounts] = useState<AccountInfo[]>([]);
    const [activeId, setActiveId] = useState("");
    const { state } = useSidebar();
    const setSelectedAccount = useChatStore((s) => s.setSelectedAccount);
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
            
            // 检查是否有持久化保存的账号
            const { selectedAccount: savedAccount } = useChatStore.getState();
                console.log("已保存的账号:", savedAccount);
            // 如果有已保存的账号且仍然存在于账户列表中，优先使用
            if (savedAccount && response.accounts.length > 0) {
                const stillExists = response.accounts.find(a => a.accountId === savedAccount.accountId);
                if (stillExists) {
                    setActiveId(savedAccount.accountId);
                    setSelectedAccount(savedAccount);
                    console.log("恢复已选账号:", savedAccount);
                    return;
                }
            }
            
            
            // 如果没有已保存的账号，自动选择第一个
            if (response.accounts.length > 0 && !savedAccount) {
                const firstAccount = response.accounts[0];
                setActiveId(firstAccount.accountId);
                setSelectedAccount(firstAccount);
                console.log("自动选择首个账户:", firstAccount);
            }
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
                                onClick={(accountId) => {
                                    setActiveId(accountId);
                                    setSelectedAccount(account);
                                    console.log("切换账号:", account);
                                }}
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
                        <TooltipContent side="right" className="cursor-pointer font-medium text-white">
                            <p>添加账户</p>
                        </TooltipContent>
                    )}
                </Tooltip>
            </SidebarFooter>

            {/* 二维码登录对话框组件 */}
            <QRCodeLoginDialog
             open={isDialogOpen}
             onOpenChange={setIsDialogOpen}
             onLoginSuccess={handleLoginSuccess}
           />
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
