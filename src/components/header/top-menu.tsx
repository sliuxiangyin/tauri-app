import { useTabStore } from "@/stores/useTabStore"
import { Tabs, TabsList, TabsTrigger } from "../ui/tabs"
import { useEffect, useState } from "react"
import { DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuGroup, DropdownMenuLabel, DropdownMenuItem } from "../ui/dropdown-menu"
import { useNavigate } from "react-router-dom"
import { cn } from "@/lib/utils"
import { Button } from "../ui/button"
import { HoverCard, HoverCardTrigger, HoverCardContent } from "../ui/hover-card"
export const TopMenu = () => {
    const [hoverCardOpen, setHoverCardOpen] = useState(false)
    const navigate = useNavigate();
    const currentTab = useTabStore((state) => state.currentTab)
    const setCurrentTab = useTabStore((state) => state.setCurrentTab)
    // 切换时的回调函数
    const handleTabChange = (value: string) => {
        if (value === "settings") return
        setCurrentTab(value)   // 更新全局状态
    }
    useEffect(() => {
        console.log("当前选中选项卡：", currentTab)
    }, [currentTab])
    return (
        <Tabs value={currentTab} onValueChange={handleTabChange}>
            <TabsList>
                <TabsTrigger value="home" className="w-40">首页</TabsTrigger>
                <TabsTrigger value="settings" className="w-40">
                    <HoverCard open={hoverCardOpen} onOpenChange={setHoverCardOpen} openDelay={10} closeDelay={100}>
                        <HoverCardTrigger asChild>
                            <div className="w-40">
                                更多
                            </div>
                        </HoverCardTrigger>
                        <HoverCardContent className="flex w-64 flex-col gap-0.5">
                            <Button variant="ghost" onClick={() => { setHoverCardOpen(false); setCurrentTab("settings"); navigate('/model-config'); }}>
                                模型配置
                            </Button>
                            <Button variant="ghost" onClick={() => { setHoverCardOpen(false); setCurrentTab("settings"); navigate('/mcp-page'); }}>
                                MCP服务
                            </Button>
                            <Button variant="ghost" onClick={() => { setHoverCardOpen(false); setCurrentTab("settings"); navigate('/setting'); }}>
                                其它设置
                            </Button>
                        </HoverCardContent>
                    </HoverCard>
                </TabsTrigger>
            </TabsList>
        </Tabs>
    )
}