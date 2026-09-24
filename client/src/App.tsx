import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import "./styles/index.css";
import { ThemeProvider } from "./theme";
import { Rail } from "./components/Rail";
import { OPEN_WHILE_LOCKED, PLACES, type Place } from "./places";
import { CaptureScreen } from "./screens/CaptureScreen";
import { TodayScreen } from "./screens/TodayScreen";
import { NotepadScreen } from "./screens/NotepadScreen";
import { CaptureDetailScreen } from "./screens/CaptureDetailScreen";
import { EntityDetailScreen } from "./screens/EntityDetailScreen";
import { ReviewScreen } from "./screens/ReviewScreen";
import { BriefScreen } from "./screens/BriefScreen";
import { SearchScreen } from "./screens/SearchScreen";
import { RelationsScreen } from "./screens/RelationsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { LockGate } from "./components/LockGate";
import { Sheet } from "./components/Sheet";
import { useEdgeSwipeBack, usePhoneLayout, useReading } from "./phone";
import {
  lockStatus,
  onLocked,
  onOpenBrief,
  onOpenReview,
  onRecordRequest,
  onReviewDue,
  onSummonCapture,
  onUnlocked,
  onWriteNote,
  type LockStatus,
} from "./desktop";

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
  | { kind: "entity"; id: string }
  /// A week looked back on. Not a sixth place in the rail: the five are a
  /// decision (docs/design.md), and a week is something you arrive at —
  /// from Today, the tray, or the weekly notification.
  | { kind: "review"; week?: string }
  /// A meeting and what you know about it — from the lit menu bar, or
  /// from Today. Like the review, something you arrive at, not a place.
  | { kind: "brief"; key: string };

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

  /// Unlocked from the menu-bar menu: the gate here goes too, rather than
  /// asking for Touch ID a second time. And "Write a Note…" from the same
  /// menu opens the capture sheet — without bumping `summons`, which is
  /// what would start the microphone.
  useEffect(() => {
    const offs: Promise<() => void>[] = [
      onUnlocked((status) => setLock(status)),
      onWriteNote(() =>
        setStack((s) => [...s.filter((v) => v.kind !== "capture"), { kind: "capture" }]),
      ),
    ];
    return () => offs.forEach((off) => off.then((stop) => stop()));
  }, []);

  useEffect(
    () =>
      onSummonCapture(() => {
        setStack((s) => [...s.filter((v) => v.kind !== "capture"), { kind: "capture" }]);
        setSummons((n) => n + 1);
      }),
    [],
  );

  /// The phone's Action Button (docs/iphone.md, I5): the same as the
  /// Mac's shortcut — open the capture sheet and start recording.
  useEffect(
    () =>
      onRecordRequest(() => {
        setStack((s) => [...s.filter((v) => v.kind !== "capture"), { kind: "capture" }]);
        setSummons((n) => n + 1);
      }),
    [],
  );

  /// The weekly review, announced by the notification or asked for from
  /// the tray. Announced, it opens the next time the window is in front
  /// rather than jumping in front of whatever is already there — and it
  /// never lands on top of a capture in progress: speaking comes first
  /// (ADR 0012), so it waits for the next time the window comes forward.
  useEffect(() => {
    let pending = false;
    let live = true;
    const offs: (() => void)[] = [];

    const open = () =>
      setStack((s) => {
        const top = s[s.length - 1];
        if (top?.kind === "capture") {
          pending = true;
          return s;
        }
        if (top?.kind === "review") return s;
        return [...s, { kind: "review" }];
      });
    const onFocus = () => {
      if (!pending) return;
      pending = false;
      open();
    };
    window.addEventListener("focus", onFocus);

    const keep = (listening: Promise<() => void>) =>
      listening.then((off) => (live ? offs.push(off) : off()));
    keep(
      onReviewDue(() => {
        if (document.hasFocus()) open();
        else pending = true;
      }),
    );
    keep(onOpenReview(open));

    return () => {
      live = false;
      window.removeEventListener("focus", onFocus);
      offs.forEach((off) => off());
    };
  }, []);

  /// The lit menu bar was clicked. Same rule as the review: never on top
  /// of a capture in progress — speaking comes first.
  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    let pending: string | null = null;
    const open = (key: string) =>
      setStack((s) => {
        const top = s[s.length - 1];
        if (top?.kind === "capture") {
          pending = key;
          return s;
        }
        if (top?.kind === "brief" && top.key === key) return s;
        return [...s.filter((v) => v.kind !== "brief"), { kind: "brief", key }];
      });
    const onFocus = () => {
      if (pending === null) return;
      const key = pending;
      pending = null;
      open(key);
    };
    window.addEventListener("focus", onFocus);
    onOpenBrief(open).then((off) => {
      if (live) stop = off;
      else off();
    });
    return () => {
      live = false;
      window.removeEventListener("focus", onFocus);
      stop?.();
    };
  }, []);

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
  const openReview = (week?: string) => setStack((s) => [...s, { kind: "review", week }]);
  const openBrief = (key: string) => setStack((s) => [...s, { kind: "brief", key }]);
  const back = useCallback(() => setStack((s) => s.slice(0, -1)), []);

  const phone = usePhoneLayout();
  const top = stack[stack.length - 1];
  /// On the phone, capturing rises as a sheet over the page you were on
  /// instead of replacing it, so the page underneath keeps being drawn.
  const sheet = phone && top?.kind === "capture";
  const view = sheet ? stack[stack.length - 2] : top;
  const depth = sheet ? stack.length - 1 : stack.length;

  // A detail view always reads something, so it is gated whatever place it
  // was opened from. Capturing is not: it only ever adds.
  const reading = view ? view.kind !== "capture" : !OPEN_WHILE_LOCKED.includes(place);
  const locked = (lock?.locked ?? false) && reading;

  const gatedName =
    view && view.kind !== "capture"
      ? "This"
      : (PLACES.find((entry) => entry.id === place)?.gatedName ?? "This");

  /// Which way the page moved, so a detail slides in from the right and
  /// going back slides the one underneath in from the left, as a
  /// navigation stack does. Only the phone animates it (phone.css).
  const lastDepth = useRef(depth);
  const motion = depth > lastDepth.current ? "push" : depth < lastDepth.current ? "pop" : "";
  useEffect(() => {
    lastDepth.current = depth;
  }, [depth]);

  const mainRef = useRef<HTMLElement>(null);
  const pageRef = useRef<HTMLDivElement>(null);
  useEdgeSwipeBack(mainRef, pageRef, phone && depth > 0 && !sheet, back);

  /// Every page starts at its top, and going back returns to where you
  /// were on the one underneath. Before this the scroll area simply kept
  /// its offset, so opening a capture from far down Today landed halfway
  /// down the capture.
  const pageKey = `${depth}-${view ? view.kind : place}`;
  const scrolled = useRef(new Map<string, number>());
  useLayoutEffect(() => {
    const el = mainRef.current;
    if (!el) return;
    el.scrollTop = motion === "pop" ? (scrolled.current.get(pageKey) ?? 0) : 0;
    const remember = () => scrolled.current.set(pageKey, el.scrollTop);
    el.addEventListener("scroll", remember, { passive: true });
    return () => el.removeEventListener("scroll", remember);
    // Only when the page itself changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pageKey]);

  const readingDown = useReading(mainRef, phone, pageKey);

  return (
    <div className={`app${readingDown ? " reading" : ""}`}>
      <Rail place={place} onGo={go} onCapture={capture} lock={lock} onLockChange={setLock} />

      {/* The graph and the notepad are full-bleed. The graph is a canvas
          you look into rather than a document you read, and the notepad is
          a desk with its own header and footer — the padding every other
          screen gets would only be a border shrinking either back into a
          box. */}
      <main
        ref={mainRef}
        className={`app-main${!view && (place === "graph" || place === "write") ? " app-main-bleed" : ""}`}
      >
        <div
          ref={pageRef}
          key={pageKey}
          className={`page${motion ? ` page-${motion}` : ""}`}
        >
        {locked ? (
          <LockGate status={lock!} what={gatedName} onUnlocked={setLock} />
        ) : view?.kind === "capture" ? (
          <CaptureScreen
            summons={summons}
            onOpenCapture={openCapture}
            onOpenEntity={openEntity}
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
        ) : view?.kind === "brief" ? (
          <BriefScreen
            key={view.key}
            meetingKey={view.key}
            onOpenCapture={openCapture}
            onOpenEntity={openEntity}
            onBack={back}
          />
        ) : view?.kind === "review" ? (
          <ReviewScreen
            key={view.week ?? "this"}
            week={view.week}
            onOpenCapture={openCapture}
            onOpenEntity={openEntity}
            onBack={back}
          />
        ) : (
          <Here
            place={place}
            onOpenCapture={openCapture}
            onOpenEntity={openEntity}
            onOpenReview={openReview}
            onOpenBrief={openBrief}
            onGo={go}
          />
        )}
        </div>
      </main>

      {sheet && (
        <Sheet title="New Capture" onClose={back}>
          {(close) => (
            <CaptureScreen
              summons={summons}
              onOpenCapture={openCapture}
              onOpenEntity={openEntity}
              onClose={close}
              locked={lock?.locked ?? false}
              focusOnOpen={false}
            />
          )}
        </Sheet>
      )}
    </div>
  );
}

function Here({
  place,
  onOpenCapture,
  onOpenEntity,
  onOpenReview,
  onOpenBrief,
  onGo,
}: {
  place: Place;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  onOpenReview: () => void;
  onOpenBrief: (key: string) => void;
  onGo: (place: Place) => void;
}) {
  switch (place) {
    case "today":
      return (
        <TodayScreen
          onOpenCapture={onOpenCapture}
          onOpenEntity={onOpenEntity}
          onOpenReview={onOpenReview}
          onOpenBrief={onOpenBrief}
          onGo={onGo}
        />
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
