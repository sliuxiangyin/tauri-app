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
    }, [currentTab]);
    return (
        <div className="flex h-full w-full flex-col overflow-hidden">
            <MainHeaderWithControls />
            <div className="flex flex-1 min-h-0 bg-[#f5f5f5] overflow-hidden">

                {/* home 始终挂载保持状态；settings 条件渲染，进入时才加载 */}
                <div style={{ display: currentTab === "home" ? "flex" : "none", height: "100%", width: "100%" }}>
                    <SidebarProvider defaultOpen={false} style={{ minHeight: "100%" } as React.CSSProperties}>
                        <AppSidebar />
                        <div className="flex h-full w-full flex-col overflow-hidden rounded-[12px]">
                            <ChatLayout />
                        </div>
                    </SidebarProvider>
                </div>
                {currentTab === "settings" && (
                    <div className="flex h-full w-full">
                        <DashboardPage />
                    </div>
                )}

            </div>
        </div>
    );
}

export default MainPage;