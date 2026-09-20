import { useEffect, useState } from "react";
import "./App.css";
import { ThemeProvider } from "./theme";
import { Sidebar } from "./components/Sidebar";
import { CaptureScreen } from "./screens/CaptureScreen";
import { CaptureDetailScreen } from "./screens/CaptureDetailScreen";
import { TimelineScreen } from "./screens/TimelineScreen";
import { SearchScreen } from "./screens/SearchScreen";
import { EntitiesScreen } from "./screens/EntitiesScreen";
import { RelationsScreen } from "./screens/RelationsScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { onSummonCapture } from "./desktop";

export type TabId = "capture" | "timeline" | "search" | "entities" | "relations" | "chat" | "settings";

function Shell() {
  const [activeTab, setActiveTab] = useState<TabId>("capture");
  // Bumped every time the global shortcut fires. CaptureScreen watches it
  // to clear itself and take focus, so the shortcut always lands on an
  // empty field even if the last capture is still on screen.
  const [summons, setSummons] = useState(0);
  // The capture whose detail view is open, layered over the active tab.
  // Not a tab of its own: you always arrive at a detail *from* somewhere,
  // and going back should return you there.
  const [openCapture, setOpenCapture] = useState<string | null>(null);

  useEffect(
    () =>
      onSummonCapture(() => {
        setOpenCapture(null);
        setActiveTab("capture");
        setSummons((n) => n + 1);
      }),
    [],
  );

  function selectTab(tab: TabId) {
    setOpenCapture(null);
    setActiveTab(tab);
  }

  return (
    <div className="app-shell">
      <Sidebar activeTab={activeTab} onSelectTab={selectTab} />
      <main className="app-content">
        {openCapture ? (
          <CaptureDetailScreen
            eventId={openCapture}
            onOpen={setOpenCapture}
            onBack={() => setOpenCapture(null)}
          />
        ) : (
          <Screen tab={activeTab} summons={summons} onOpenCapture={setOpenCapture} />
        )}
      </main>
    </div>
  );
}

function Screen({
  tab,
  summons,
  onOpenCapture,
}: {
  tab: TabId;
  summons: number;
  onOpenCapture: (eventId: string) => void;
}) {
  switch (tab) {
    case "capture":
      return <CaptureScreen summons={summons} onOpenCapture={onOpenCapture} />;
    case "timeline":
      return <TimelineScreen onOpenCapture={onOpenCapture} />;
    case "search":
      return <SearchScreen onOpenCapture={onOpenCapture} />;
    case "entities":
      return <EntitiesScreen />;
    case "relations":
      return <RelationsScreen />;
    case "chat":
      return <ChatScreen />;
    case "settings":
      return <SettingsScreen />;
  }
}

function App() {
  return (
    <ThemeProvider>
      <Shell />
    </ThemeProvider>
  );
}

export default App;
