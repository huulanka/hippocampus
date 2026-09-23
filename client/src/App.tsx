import { useEffect, useState } from "react";
import "./styles/index.css";
import { ThemeProvider } from "./theme";
import { Rail } from "./components/Rail";
import { OPEN_WHILE_LOCKED, PLACES, type Place } from "./places";
import { CaptureScreen } from "./screens/CaptureScreen";
import { TodayScreen } from "./screens/TodayScreen";
import { NotepadScreen } from "./screens/NotepadScreen";
import { CaptureDetailScreen } from "./screens/CaptureDetailScreen";
import { EntityDetailScreen } from "./screens/EntityDetailScreen";
import { SearchScreen } from "./screens/SearchScreen";
import { RelationsScreen } from "./screens/RelationsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { LockGate } from "./components/LockGate";
import { lockStatus, onLocked, onSummonCapture, type LockStatus } from "./desktop";

/// Something you are looking at, layered over whichever place you were in.
///
/// None of these is a place. You always arrive at one *from* somewhere,
/// and following a capture to an entity to another capture has to be
/// retraceable — hence a stack rather than a single slot.
///
/// `capture` is on this list and not in the rail on purpose: the shortcut
/// is the record button (ADR 0012), so speaking opens over whatever you
/// were already doing and closes back onto it. Making it a tab meant the
/// one thing you do twenty times a day was also the one thing that threw
/// away where you were.
type View =
  | { kind: "capture" }
  | { kind: "captureDetail"; id: string }
  | { kind: "entity"; id: string };

function Shell() {
  const [place, setPlace] = useState<Place>("today");
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
        setStack((s) => [...s.filter((v) => v.kind !== "capture"), { kind: "capture" }]);
        setSummons((n) => n + 1);
      }),
    [],
  );

  function go(next: Place) {
    setStack([]);
    setPlace(next);
  }

  /// Opening the capture sheet, from the rail or from the shortcut. One
  /// at a time: pressing it twice should leave one sheet open, and
  /// closing it should land back where you actually were.
  const capture = () =>
    setStack((s) => [...s.filter((v) => v.kind !== "capture"), { kind: "capture" }]);

  const openCapture = (id: string) => setStack((s) => [...s, { kind: "captureDetail", id }]);
  const openEntity = (id: string) => setStack((s) => [...s, { kind: "entity", id }]);
  const back = () => setStack((s) => s.slice(0, -1));

  const view = stack[stack.length - 1];
  // A detail view always reads something, so it is gated whatever place it
  // was opened from. Capturing is not: it only ever adds.
  const reading = view ? view.kind !== "capture" : !OPEN_WHILE_LOCKED.includes(place);
  const locked = (lock?.locked ?? false) && reading;

  const gatedName =
    view && view.kind !== "capture"
      ? "This"
      : (PLACES.find((entry) => entry.id === place)?.gatedName ?? "This");

  return (
    <div className="app">
      <Rail place={place} onGo={go} onCapture={capture} lock={lock} onLockChange={setLock} />

      {/* The graph and the notepad are full-bleed. The graph is a canvas
          you look into rather than a document you read, and the notepad is
          a desk with its own header and footer — the padding every other
          screen gets would only be a border shrinking either back into a
          box. */}
      <main
        className={`app-main${!view && (place === "graph" || place === "write") ? " app-main-bleed" : ""}`}
      >
        {locked ? (
          <LockGate status={lock!} what={gatedName} onUnlocked={setLock} />
        ) : view?.kind === "capture" ? (
          <CaptureScreen
            summons={summons}
            onOpenCapture={openCapture}
            onClose={back}
            locked={lock?.locked ?? false}
          />
        ) : view?.kind === "captureDetail" ? (
          <CaptureDetailScreen
            key={view.id}
            eventId={view.id}
            onOpenCapture={openCapture}
            onOpenEntity={openEntity}
            onBack={back}
          />
        ) : view?.kind === "entity" ? (
          <EntityDetailScreen
            key={view.id}
            entityId={view.id}
            onOpenEntity={openEntity}
            onOpenCapture={openCapture}
            onBack={back}
          />
        ) : (
          <Here place={place} onOpenCapture={openCapture} onOpenEntity={openEntity} onGo={go} />
        )}
      </main>
    </div>
  );
}

function Here({
  place,
  onOpenCapture,
  onOpenEntity,
  onGo,
}: {
  place: Place;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  onGo: (place: Place) => void;
}) {
  switch (place) {
    case "today":
      return (
        <TodayScreen onOpenCapture={onOpenCapture} onOpenEntity={onOpenEntity} onGo={onGo} />
      );
    case "write":
      return <NotepadScreen onOpenCapture={onOpenCapture} />;
    case "graph":
      return <RelationsScreen onOpenEntity={onOpenEntity} />;
    case "search":
      return <SearchScreen onOpenCapture={onOpenCapture} onOpenEntity={onOpenEntity} />;
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
