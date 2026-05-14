import {BaseHeader} from "@/components/header/base-header.tsx";
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger
} from "@/components/ui/dropdown-menu.tsx";
import {Button} from "@/components/ui/button.tsx";
import {Settings} from "lucide-react";
import {WebviewWindow} from "@tauri-apps/api/webviewWindow";

export const MainHeaderWithControls = () => {
    const gotoModelConfig = () => {
        try {
            const settingsWindow = new WebviewWindow('model-config', {
                url: '#/model-config',
                title: '设置',
                width: 800,
                height: 600,
                center: true,
                decorations: false,
                transparent: true,
            })

            // 监听创建成功
            settingsWindow.once('tauri://created', () => {
                console.log('窗口创建成功')
            })

            // 监听创建错误
            settingsWindow.once('tauri://error', (e) => {
                console.error('窗口创建失败:', e)
            })

            // 监听窗口关闭
            settingsWindow.once('tauri://close-requested', () => {
                console.log('窗口关闭请求')
                settingsWindow.close();
            })

            return settingsWindow
        } catch (error) {
            console.error('创建窗口异常:', error)
        }
    }
    return <BaseHeader title="ChatGPT" titleCentered={false}>
        <DropdownMenu>
            <DropdownMenuTrigger asChild>
                <Button variant="outline">
                    <Settings className="h-4 w-4" />
                </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent>
                <DropdownMenuItem onClick={gotoModelConfig}>模型配置</DropdownMenuItem>
                <DropdownMenuItem>Billing</DropdownMenuItem>
                <DropdownMenuItem>Settings</DropdownMenuItem>
            </DropdownMenuContent>
        </DropdownMenu>
    </BaseHeader>
}