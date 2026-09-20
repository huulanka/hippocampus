import { useEffect, useState } from "react";
import "./App.css";
import { ThemeProvider } from "./theme";
import { Sidebar } from "./components/Sidebar";
import { CaptureScreen } from "./screens/CaptureScreen";
import { CaptureDetailScreen } from "./screens/CaptureDetailScreen";
import { EntityDetailScreen } from "./screens/EntityDetailScreen";
import { TimelineScreen } from "./screens/TimelineScreen";
import { SearchScreen } from "./screens/SearchScreen";
import { EntitiesScreen } from "./screens/EntitiesScreen";
import { RelationsScreen } from "./screens/RelationsScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { onSummonCapture } from "./desktop";

export type TabId = "capture" | "timeline" | "search" | "entities" | "relations" | "chat" | "settings";

/// A thing being looked at, layered over whichever tab you were on.
/// Details are not tabs: you always arrive at one *from* somewhere, and
/// following a capture to an entity to another capture has to be
/// retraceable — hence a stack rather than a single slot.
type View = { kind: "capture"; id: string } | { kind: "entity"; id: string };

function Shell() {
  const [activeTab, setActiveTab] = useState<TabId>("capture");
  // Bumped every time the global shortcut fires. CaptureScreen watches it
  // to clear itself and take focus, so the shortcut always lands on an
  // empty field even if the last capture is still on screen.
  const [summons, setSummons] = useState(0);
  const [stack, setStack] = useState<View[]>([]);

  useEffect(
    () =>
      onSummonCapture(() => {
        setStack([]);
        setActiveTab("capture");
        setSummons((n) => n + 1);
      }),
    [],
  );

  function selectTab(tab: TabId) {
    setStack([]);
    setActiveTab(tab);
  }

  const openCapture = (id: string) => setStack((s) => [...s, { kind: "capture", id }]);
  const openEntity = (id: string) => setStack((s) => [...s, { kind: "entity", id }]);
  const back = () => setStack((s) => s.slice(0, -1));

  const current = stack[stack.length - 1];

  return (
    <div className="app-shell">
      <Sidebar activeTab={activeTab} onSelectTab={selectTab} />
      <main className="app-content">
        {current?.kind === "capture" ? (
          <CaptureDetailScreen
            key={current.id}
            eventId={current.id}
            onOpenCapture={openCapture}
            onOpenEntity={openEntity}
            onBack={back}
          />
        ) : current?.kind === "entity" ? (
          <EntityDetailScreen
            key={current.id}
            entityId={current.id}
            onOpenEntity={openEntity}
            onOpenCapture={openCapture}
            onBack={back}
          />
        ) : (
          <Screen
            tab={activeTab}
            summons={summons}
            onOpenCapture={openCapture}
            onOpenEntity={openEntity}
          />
        )}
      </main>
    </div>
  );
}

function Screen({
  tab,
  summons,
  onOpenCapture,
  onOpenEntity,
}: {
  tab: TabId;
  summons: number;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
}) {
  switch (tab) {
    case "capture":
      return <CaptureScreen summons={summons} onOpenCapture={onOpenCapture} />;
    case "timeline":
      return <TimelineScreen onOpenCapture={onOpenCapture} />;
    case "search":
      return <SearchScreen onOpenCapture={onOpenCapture} onOpenEntity={onOpenEntity} />;
    case "entities":
      return <EntitiesScreen onOpenEntity={onOpenEntity} />;
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
