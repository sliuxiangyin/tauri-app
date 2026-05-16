import "./App.css";
import { TooltipProvider } from "./components/ui/tooltip";
import MainPage from "./pages/main-page";
 
function App() {
  

  return (
    <TooltipProvider delayDuration={0}>
      <div className="flex h-full w-full flex-col overflow-hidden rounded-[12px]">
          <MainPage />
      </div>
    </TooltipProvider>
  );
}

export default App;