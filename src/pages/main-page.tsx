import ChatLayout from "@/components/chat/chat-shell.tsx";
import { MainHeaderWithControls } from "../components/header/main-header-with-controls.tsx";
import { SidebarProvider } from "@/components/ui/sidebar";
import AppSidebar from "@/components/chat/account-list.tsx";

function MainPage() {
    return (
        <SidebarProvider>
            <div className="flex h-full w-full flex-col overflow-hidden">
                <MainHeaderWithControls />
                <div className="flex flex-1 min-h-0 bg-[#f5f5f5] overflow-hidden">
                    <AppSidebar />
                    <div className="flex-1 overflow-hidden">
                        <ChatLayout />
                    </div>
                </div>
            </div>
        </SidebarProvider>
    );
}

export default MainPage;