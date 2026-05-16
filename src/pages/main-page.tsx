import ChatLayout from "@/components/chat/chat-shell.tsx";
import { MainHeaderWithControls } from "../components/header/main-header-with-controls.tsx";
import { SidebarProvider } from "@/components/ui/sidebar";
import AppSidebar from "@/components/chat/account-list.tsx";
import DashboardPage from "./dashboard-page.tsx";
import { useTabStore } from "@/stores/useTabStore.ts";
import { useEffect } from "react";

function MainPage() {
    const currentTab = useTabStore((state) => state.currentTab);
    useEffect(() => {
        console.log("Main 监听到选项卡变化：", currentTab);
        // 可在此处执行其他逻辑
    }, [currentTab]);
    return (
        <SidebarProvider>
            <div className="flex h-full w-full flex-col overflow-hidden">
                <MainHeaderWithControls />
                <div className="flex flex-1 min-h-0 bg-[#f5f5f5] overflow-hidden">

                    {/* 两个页面始终渲染，通过 display 控制显示/隐藏 */}
                    <div style={{ display: currentTab === "home" ? "flex" : "none", height: "100%", width: "100%" }}>
                        <AppSidebar />
                        <div className="flex h-full w-full flex-col overflow-hidden rounded-[12px]">
                            <ChatLayout />
                        </div>
                    </div>
                    <div style={{ display: currentTab === "settings" ? "flex" : "none", height: "100%", width: "100%" }}>
                        <DashboardPage />
                    </div>

                </div>
            </div>
        </SidebarProvider>
    );
}

export default MainPage;