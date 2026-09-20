import { useState } from "react";
import "./App.css";
import { ThemeProvider } from "./theme";
import { Sidebar } from "./components/Sidebar";
import { CaptureScreen } from "./screens/CaptureScreen";
import { TimelineScreen } from "./screens/TimelineScreen";
import { SearchScreen } from "./screens/SearchScreen";
import { EntitiesScreen } from "./screens/EntitiesScreen";
import { RelationsScreen } from "./screens/RelationsScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { SettingsScreen } from "./screens/SettingsScreen";

export type TabId = "capture" | "timeline" | "search" | "entities" | "relations" | "chat" | "settings";

const SCREENS: Record<TabId, React.ComponentType> = {
  capture: CaptureScreen,
  timeline: TimelineScreen,
  search: SearchScreen,
  entities: EntitiesScreen,
  relations: RelationsScreen,
  chat: ChatScreen,
  settings: SettingsScreen,
};

function Shell() {
  const [activeTab, setActiveTab] = useState<TabId>("capture");
  const Screen = SCREENS[activeTab];

  return (
    <div className="app-shell">
      <Sidebar activeTab={activeTab} onSelectTab={setActiveTab} />
      <main className="app-content">
        <Screen />
      </main>
    </div>
  );
}

function App() {
  return (
    <ThemeProvider>
      <Shell />
    </ThemeProvider>
  );
}

export default App;
