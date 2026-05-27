import "./App.css";
import { useEffect } from "react";
import { TooltipProvider } from "./components/ui/tooltip";
import MainPage from "./pages/main-page";
import { useChatStore } from "./stores/useChatStore";

function App() {
  const initWebhookListener = useChatStore((s) => s.initWebhookListener);

  useEffect(() => {
    // 应用启动时初始化 Webhook 消息监听
    const cleanupWebhook = initWebhookListener();

    return () => {
      cleanupWebhook.then((fn) => fn());
    };
  }, [initWebhookListener]);

  return (
    <TooltipProvider delayDuration={0}>
      <div className="flex h-full w-full flex-col overflow-hidden rounded-[12px]">
          <MainPage />
      </div>
    </TooltipProvider>
  );
}

export default App;
