import { Routes, Route } from "react-router-dom";
import ModelConfigPage from "./model-config-page";
import McpPage from "./mcp-page";

function DashboardPage() {
  return (
            <div className="flex h-full w-full flex-col overflow-hidden">
                <div className="flex flex-1 min-h-0 bg-[#f5f5f5] overflow-hidden">
                     <Routes>
                                    {/* <Route path="/" element={<Navigate to="/Dashboard" replace />} /> */}
                                    <Route path="/mcp-page" element={<McpPage />} />
                                    <Route path="/model-config" element={<ModelConfigPage />} />
                        </Routes>
                </div>
            </div>
  );
}
export default DashboardPage;
