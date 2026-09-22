import { useEffect, useState } from "react";
import "./App.css";
import { ThemeProvider } from "./theme";
import { Sidebar } from "./components/Sidebar";
import { CaptureScreen } from "./screens/CaptureScreen";
import { ResurfaceScreen } from "./screens/ResurfaceScreen";
import { CaptureDetailScreen } from "./screens/CaptureDetailScreen";
import { EntityDetailScreen } from "./screens/EntityDetailScreen";
import { TimelineScreen } from "./screens/TimelineScreen";
import { SearchScreen } from "./screens/SearchScreen";
import { EntitiesScreen } from "./screens/EntitiesScreen";
import { RelationsScreen } from "./screens/RelationsScreen";
import { ChangesScreen } from "./screens/ChangesScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { LockGate } from "./components/LockGate";
import { lockStatus, onLocked, onSummonCapture, type LockStatus } from "./desktop";

export type TabId = "capture" | "resurface" | "timeline" | "search" | "entities" | "relations" | "changes" | "chat" | "settings";

/// A thing being looked at, layered over whichever tab you were on.
/// Details are not tabs: you always arrive at one *from* somewhere, and
/// following a capture to an entity to another capture has to be
/// retraceable — hence a stack rather than a single slot.
type View = { kind: "capture"; id: string } | { kind: "entity"; id: string };

/// The two places that stay open while the notes are locked.
///
/// Capture, because speaking a note only ever adds — and because the
/// global shortcut is the main path through this app, so a gate in front
/// of it would cost the one property the whole design is built on.
/// Settings, because it holds no notes, and because being shut out of the
/// screen that configures the backend by a guard you cannot reach to
/// switch off is a trap. Switching the guard off from there authenticates
/// first, so nothing is given away by letting you in.
const OPEN_WHILE_LOCKED: TabId[] = ["capture", "settings"];

/// What each gated tab calls itself on the lock screen, so it says what
/// was being asked for rather than "this content".
const TAB_NAMES: Partial<Record<TabId, string>> = {
  resurface: "What you said before",
  timeline: "Your timeline",
  search: "Search",
  entities: "The things you have mentioned",
  relations: "Your graph",
  changes: "How this got organised",
  chat: "Chat",
};

function Shell() {
  const [activeTab, setActiveTab] = useState<TabId>("capture");
  // Bumped every time the global shortcut fires. CaptureScreen watches it
  // to clear itself and take focus, so the shortcut always lands on an
  // empty field even if the last capture is still on screen.
  const [summons, setSummons] = useState(0);
  const [stack, setStack] = useState<View[]>([]);
  /// `null` until the Rust side has been asked. Rendering the app before
  /// the answer arrives would flash a screenful of notes on a machine
  /// that is about to say they are locked.
  const [lock, setLock] = useState<LockStatus | null>(null);

  useEffect(() => {
    let live = true;
    lockStatus()
      .then((status) => live && setLock(status))
      .catch(() => live && setLock(null));
    return () => {
      live = false;
    };
  }, []);

  /// The app locking itself has to take the screen away underneath
  /// whoever is not looking at it — which also unmounts the screen, so
  /// the notes it had fetched stop being in the page at all.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let live = true;
    onLocked(() => {
      setStack([]);
      setLock((previous) => (previous ? { ...previous, locked: true } : previous));
    }).then((off) => {
      if (live) stop = off;
      else off();
    });
    return () => {
      live = false;
      stop?.();
    };
  }, []);

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
  // A detail view always reads something, so it is gated whatever tab it
  // was opened from.
  const locked =
    (lock?.locked ?? false) && (current !== undefined || !OPEN_WHILE_LOCKED.includes(activeTab));

  return (
    <div className="app-shell">
      <Sidebar
        activeTab={activeTab}
        onSelectTab={selectTab}
        lock={lock}
        onLockChange={setLock}
      />
      <main className="app-content">
        {locked ? (
          <LockGate
            status={lock!}
            what={current ? "This" : (TAB_NAMES[activeTab] ?? "This")}
            onUnlocked={setLock}
          />
        ) : current?.kind === "capture" ? (
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
            locked={lock?.locked ?? false}
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
  locked,
}: {
  tab: TabId;
  summons: number;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  locked: boolean;
}) {
  switch (tab) {
    case "capture":
      return (
        <CaptureScreen summons={summons} onOpenCapture={onOpenCapture} locked={locked} />
      );
    case "resurface":
      return <ResurfaceScreen onOpenCapture={onOpenCapture} onOpenEntity={onOpenEntity} />;
    case "timeline":
      return <TimelineScreen onOpenCapture={onOpenCapture} />;
    case "search":
      return <SearchScreen onOpenCapture={onOpenCapture} onOpenEntity={onOpenEntity} />;
    case "entities":
      return <EntitiesScreen onOpenEntity={onOpenEntity} />;
    case "relations":
      return <RelationsScreen onOpenEntity={onOpenEntity} />;
    case "changes":
      return <ChangesScreen onOpenEntity={onOpenEntity} />;
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
