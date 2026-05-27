
import React from "react";
import { ChevronRight, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip";
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { AccountInfo } from "@/lib/api/wechat";

interface AccountItemProps {
    account: AccountInfo;
    isActive: boolean;
    isCollapsed: boolean;
    onClick: (accountId: string) => void;
    onDelete?: (accountId: string) => void;
}

const getInitials = (name: string) => {
    return name
        .split(" ")
        .map((n) => n[0])
        .join("")
        .toUpperCase()
        .substring(0, 2);
};

const getAvatarColor = (name: string) => {
    const colors = [
        "bg-red-500",
        "bg-blue-500",
        "bg-green-500",
        "bg-yellow-500",
        "bg-purple-500",
        "bg-pink-500",
        "bg-indigo-500",
        "bg-teal-500",
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
        hash = name.charCodeAt(i) + ((hash << 5) - hash);
    }
    return colors[Math.abs(hash) % colors.length];
};

const AccountItem: React.FC<AccountItemProps> = ({
    account,
    isActive,
    isCollapsed,
    onClick,
    onDelete,
}) => {
    const handleClick = () => {
        onClick(account.accountId);
    };

    const handleDelete = () => {
        onDelete?.(account.accountId);
    };

    const content = (
        <button
            onClick={handleClick}
            className={cn(
                "w-full flex items-center gap-3 p-2.5 rounded-lg text-left transition-all duration-200",
                "hover:bg-accent hover:text-accent-foreground",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                isActive
                    ? "bg-accent text-accent-foreground shadow-sm"
                    : "text-foreground",
                isCollapsed && "justify-center p-2"
            )}
        >
            <Avatar className={cn("h-9 w-9 rounded-xl", isCollapsed && "h-10 w-10")}>
                <AvatarFallback
                    className={cn(
                        "text-white font-medium text-sm rounded-xl",
                        getAvatarColor(account.accountId)
                    )}
                >
                    {getInitials(account.accountId)}
                </AvatarFallback>
            </Avatar>

            {!isCollapsed && (
                <>
                    <div className="flex-1 min-w-0">
                        <p className="text-sm font-medium truncate">
                            {account.accountId}
                        </p>
                    </div>
                    <ChevronRight
                        className="h-4 w-4 text-muted-foreground opacity-50 flex-shrink-0" />
                </>
            )}
        </button>
    );

    return (
        <ContextMenu>
            <TooltipProvider delayDuration={0}>
                <Tooltip>
                    <ContextMenuTrigger asChild>
                        <TooltipTrigger asChild>
                            {content}
                        </TooltipTrigger>
                    </ContextMenuTrigger>
                    {isCollapsed && (
                        <TooltipContent side="right" className="font-medium">
                            <p className="text-white">{account.accountId}</p>
                        </TooltipContent>
                    )}
                </Tooltip>
            </TooltipProvider>
            <ContextMenuContent>
                <ContextMenuItem variant="destructive" onClick={handleDelete}>
                    <Trash2 />
                    删除
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    );
};

export default AccountItem;
