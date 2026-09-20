import { useEffect, useState } from "react";
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
import { onSummonCapture } from "./desktop";

export type TabId = "capture" | "timeline" | "search" | "entities" | "relations" | "chat" | "settings";

const SCREENS: Record<Exclude<TabId, "capture">, React.ComponentType> = {
  timeline: TimelineScreen,
  search: SearchScreen,
  entities: EntitiesScreen,
  relations: RelationsScreen,
  chat: ChatScreen,
  settings: SettingsScreen,
};

function Shell() {
  const [activeTab, setActiveTab] = useState<TabId>("capture");
  // Bumped every time the global shortcut fires. CaptureScreen watches it
  // to clear itself and take focus, so the shortcut always lands on an
  // empty field even if the last capture is still on screen.
  const [summons, setSummons] = useState(0);

  useEffect(
    () =>
      onSummonCapture(() => {
        setActiveTab("capture");
        setSummons((n) => n + 1);
      }),
    [],
  );

  const Screen = activeTab === "capture" ? null : SCREENS[activeTab];

  return (
    <div className="app-shell">
      <Sidebar activeTab={activeTab} onSelectTab={setActiveTab} />
      <main className="app-content">
        {Screen ? <Screen /> : <CaptureScreen summons={summons} />}
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
