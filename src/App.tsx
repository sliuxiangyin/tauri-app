import "./App.css";
import { Navigate, Route, Routes } from "react-router-dom";
import { TooltipProvider } from "./components/ui/tooltip";
import MainPage from "./pages/main-page";
import SettingPage from "./pages/setting-page";
import ModelConfigPage from "./pages/model-config-page";
function App() {

    

    return (
        <TooltipProvider delayDuration={0}>
                            <div className="flex h-full w-full flex-col overflow-hidden">
                                <Routes>
                                    <Route path="/" element={<Navigate to="/main" replace />} />
                                    <Route path="/main" element={<MainPage />} />
                                    {/*<Route path="/chat" element={<ChatPage />} />*/}
                                    <Route path="/setting" element={<SettingPage />} />
                                    <Route path="/model-config" element={<ModelConfigPage />} />
                                </Routes>
                            </div>
        </TooltipProvider>
    );
}

export default App;