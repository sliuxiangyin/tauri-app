// CustomHeader.tsx
import React, { useState, useEffect, ReactNode } from "react";
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
    Minus,
    Square,
    SquareStack,
    X,
    Circle
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { TopMenu } from "./top-menu";

// 定义组件的 Props 类型
interface BaseHeaderProps {
    // Logo 区域的自定义内容
    logo?: ReactNode;
    // 标题区域的自定义内容（可以是文本或组件）
    title?: ReactNode;
    // 标题是否居中（当为 true 时，标题会居中显示）
    titleCentered?: boolean;
    // 自定义窗口控制按钮区域（完全替换默认的按钮组）
    customWindowControls?: ReactNode;
    // 右侧自定义内容（完全自定义，可以是任何组件）
    children?: ReactNode;
}

export const BaseHeader = ({
                               logo,
                               title = "我的应用",
                               titleCentered = false,
                               customWindowControls,
                               children
                           }: BaseHeaderProps) => {
    const [isMaximized, setIsMaximized] = useState(false);
    const [isFocused, setIsFocused] = useState(true);
    const appWindow = getCurrentWindow();

    useEffect(() => {
        // 监听窗口最大化状态变化
        const unlistenResize = appWindow.onResized(async () => {
            const max = await appWindow.isMaximized();
            setIsMaximized(max);
        });

        // 监听窗口焦点状态
        const unlistenFocus = appWindow.onFocusChanged(({ payload: focused }) => {
            setIsFocused(focused);
        });

        // 初始化状态
        appWindow.isMaximized().then(setIsMaximized);
        appWindow.isFocused().then(setIsFocused);

        return () => {
            unlistenResize.then(fn => fn());
            unlistenFocus.then(fn => fn());
        };
    }, [appWindow]);

    const handleMinimize = async () => {
        await appWindow.minimize();
    };

    const handleMaximize = async () => {
        if (isMaximized) {
            await appWindow.unmaximize();
        } else {
            await appWindow.maximize();
        }
    };

    const handleClose = async (e?: React.MouseEvent<HTMLButtonElement>) => {
        e?.stopPropagation(); // 阻止事件冒泡
        e?.preventDefault();  // 阻止默认行为
            const currentWindow = getCurrentWindow();
            await currentWindow.setFocus();
            await currentWindow.close();
    };

    // 默认的窗口控制按钮
    const defaultWindowControls = (
        <>
            <Button
                variant="ghost"
                className="h-8 w-8 rounded-md hover:bg-muted transition-colors"
                onClick={handleMinimize}
            >
                <Minus className="h-4 w-4" />
            </Button>

            <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 rounded-md hover:bg-muted transition-colors"
                onClick={handleMaximize}
            >
                {isMaximized ? (
                    <SquareStack className="h-4 w-4" />
                ) : (
                    <Square className="h-4 w-4" />
                )}
            </Button>

            <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 rounded-md hover:bg-destructive hover:text-destructive-foreground transition-colors"
                onMouseDown ={handleClose}
            >
                <X className="h-4 w-4" />
            </Button>
        </>
    );

    // 渲染标题区域
    const renderTitle = () => {
        if (typeof title === 'string') {
            return (
                <span
                    className={cn(
                        "text-sm font-medium transition-colors",
                        isFocused ? "text-foreground" : "text-muted-foreground",
                        titleCentered && "absolute left-1/2 -translate-x-1/2"
                    )}
                    data-tauri-drag-region
                >
                    {title}
                </span>
            );
        }

        return (
            <div
                className={cn(
                    titleCentered && "absolute left-1/2 -translate-x-1/2"
                )}
                data-tauri-drag-region
            >
                {title}
            </div>
        );
    };

    return (
        <div
            className={cn(
                "bg-white relative flex items-center justify-between h-12 px-4 border-b transition-colors duration-200",
                // isFocused
                //     ? "bg-background border-border"
                //     : "bg-muted/50 border-muted"
            )}
            data-tauri-drag-region
        >
            {/* 左侧区域 - Logo */}
            <div className="flex items-center gap-2" data-tauri-drag-region>
                {logo || (
                    <div className="flex items-center justify-center w-6 h-6 rounded-md bg-gradient-to-br from-purple-500 to-blue-500">
                        <Circle className="w-3 h-3 text-white" />
                    </div>
                )}
                  {titleCentered ?<></>: (
                <div className="" data-tauri-drag-region>
                    {renderTitle()}
                </div>
            )}
            </div>
           
            {/* 标题区域 */}
            {titleCentered ? 
               ( <div className="flex items-center " data-tauri-drag-region>
                    {renderTitle()}
                </div>):( <TopMenu />)
            }

            {/* 右侧区域 - 完全自定义 */}
            <div className="flex items-center gap-1"
                 style={{ pointerEvents: 'auto' }}  // 确保可点击
                 onClick={(e) => e.stopPropagation()}  // 阻止冒泡
                 >
                {children}
                {customWindowControls || defaultWindowControls}
            </div>
        </div>
    );
};