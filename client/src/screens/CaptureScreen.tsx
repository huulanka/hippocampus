import { useEffect, useRef, useState } from "react";
import { Mascot } from "../mascot";
import { createCapture, type EchoItem } from "../api";
import {
  DEFAULT_CAPTURE_SHORTCUT,
  cancelRecording,
  formatAccelerator,
  getSettings,
  hideWindow,
  runningInDesktopApp,
  speechAvailable,
  startRecording,
  stopRecording,
} from "../desktop";

/// Identifies where a capture came from. Recorded verbatim on the event as
/// its `source`, so later captures from the phone or a recorder stay
/// distinguishable from these.
const DEVICE = "mac-desktop";

function formatElapsed(seconds: number): string {
  const m = Math.floor(seconds / 60)
    .toString()
    .padStart(2, "0");
  const s = (seconds % 60).toString().padStart(2, "0");
  return `${m}:${s}`;
}

function relativeDay(iso: string): string {
  const date = new Date(iso);
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOf(new Date()) - startOf(date)) / 86_400_000);
  if (days === 0) return "earlier today";
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 60) return `${Math.round(days / 7)} weeks ago`;
  return date.toLocaleDateString(undefined, { month: "long", year: "numeric" });
}

type Saved = { transcript: string; echo: EchoItem[]; spoken: boolean };
type Phase = "idle" | "recording" | "working";

export function CaptureScreen({ summons = 0 }: { summons?: number }) {
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [elapsed, setElapsed] = useState(0);
  const [saved, setSaved] = useState<Saved | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [canSpeak, setCanSpeak] = useState(false);
  const [shortcut, setShortcut] = useState(formatAccelerator(DEFAULT_CAPTURE_SHORTCUT));
  const inputRef = useRef<HTMLTextAreaElement>(null);
  // The summons effect fires from outside React's render cycle, so it
  // reads these rather than the captured values, which would be stale.
  const phaseRef = useRef(phase);
  const canSpeakRef = useRef(canSpeak);
  phaseRef.current = phase;
  canSpeakRef.current = canSpeak;

  useEffect(() => {
    speechAvailable().then(setCanSpeak);
    // Read rather than assumed: the shortcut is configurable, and a hint
    // naming the wrong keys is worse than no hint at all.
    getSettings().then((s) => setShortcut(formatAccelerator(s.capture_shortcut)));
  }, []);

  // The shortcut *is* the record button: pressing it starts recording,
  // pressing it again finishes. Speaking is the main path through this
  // app, so the fastest gesture should reach it directly rather than open
  // a window and wait for a second decision.
  useEffect(() => {
    if (summons === 0) return;

    if (phaseRef.current === "recording") {
      void finishRecording();
      return;
    }
    if (phaseRef.current === "working") return;

    setSaved(null);
    setError(null);
    setText("");

    if (canSpeakRef.current) void beginRecording();
    else inputRef.current?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [summons]);

  useEffect(() => {
    if (phase !== "recording") return;
    const id = setInterval(() => setElapsed((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, [phase]);

  /// Escape sends the window away, so the capture field behaves like a
  /// panel rather than an app you have to close. Unsaved text is kept in
  /// state, so summoning it again brings the half-finished thought back.
  function dismiss() {
    void hideWindow();
  }

  async function saveTyped() {
    const transcript = text.trim();
    if (!transcript || phase !== "idle") return;

    setPhase("working");
    setError(null);
    try {
      const accepted = await createCapture(transcript, DEVICE);
      setSaved({ transcript, echo: accepted.echo, spoken: false });
      setText("");
    } catch (err) {
      setError(String(err));
    } finally {
      setPhase("idle");
    }
  }

  async function beginRecording() {
    setError(null);
    setElapsed(0);
    try {
      await startRecording();
      setPhase("recording");
    } catch (err) {
      setError(String(err));
    }
  }

  async function finishRecording() {
    setPhase("working");
    try {
      const result = await stopRecording();
      setSaved({ transcript: result.transcript, echo: result.capture.echo, spoken: true });
      setText("");
    } catch (err) {
      setError(String(err));
    } finally {
      setPhase("idle");
    }
  }

  async function discardRecording() {
    try {
      await cancelRecording();
    } catch {
      // Nothing useful to say here: the recording is being thrown away.
    }
    setPhase("idle");
  }

  function startNext() {
    setSaved(null);
    setError(null);
    inputRef.current?.focus();
  }

  if (phase === "recording") {
    return (
      <div className="capture-idle">
        <Mascot state="listening" cell={9} />
        <h3>listening…</h3>
        <p className="dim">{formatElapsed(elapsed)}</p>
        <div className="panel transcript-panel">
          <div className="kicker">
            [ RECORDING ]{" "}
            <span className="dim">— kept as the original, transcribed on this Mac</span>
          </div>
          <p className="transcript-placeholder">
            speak freely<span className="blink-cursor">▌</span>
          </p>
        </div>
        <div className="capture-actions">
          <span className="btn" onClick={discardRecording}>
            [ Discard ]
          </span>
          <span className="btn btn-accent" onClick={finishRecording}>
            [ Done ]
          </span>
        </div>
      </div>
    );
  }

  if (saved) {
    return (
      <div className="capture-idle">
        <Mascot state="idle" cell={9} />
        <h3>kept, word for word</h3>
        <div className="panel transcript-panel">
          <div className="kicker">
            [ CAPTURED ]{" "}
            <span className="dim">
              {saved.spoken
                ? "— transcript; the recording itself is the original"
                : "— raw, verbatim, never edited"}
            </span>
          </div>
          <p className="timeline-transcript">{saved.transcript}</p>
        </div>

        {saved.echo.length > 0 ? (
          <div className="panel transcript-panel">
            <div className="kicker">
              [ YOU'VE BEEN HERE BEFORE ]{" "}
              <span className="dim">— your own earlier words, not a summary</span>
            </div>
            <div className="card-stack">
              {saved.echo.map((item) => (
                <div key={item.capture_event_id} className="timeline-card">
                  <div className="timeline-card-meta">
                    <span className="dim">// {relativeDay(item.occurred_at)}</span>
                    <span className="dim">{item.similarity.toFixed(2)}</span>
                  </div>
                  <p className="timeline-transcript">{item.transcript_text}</p>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <p className="dim">
            Nothing close enough in your earlier captures. That's the honest answer, not an
            empty one.
          </p>
        )}

        <div className="capture-actions">
          <span className="btn btn-accent" onClick={startNext}>
            [ Capture Another ]
          </span>
        </div>
      </div>
    );
  }

  const working = phase === "working";
  const desktop = runningInDesktopApp();

  return (
    <div className="capture-idle">
      <Mascot state={working ? "thinking" : "idle"} cell={9} />
      <h3>{working ? "keeping it…" : "ready when you are"}</h3>
      <p className="dim capture-lede">
        {canSpeak ? "Speak it, or type it." : "Type it."}
      </p>

      <div className="panel transcript-panel">
        <div className="kicker">
          [ CAPTURE ] <span className="dim">— stored exactly as written</span>
        </div>
        <textarea
          ref={inputRef}
          className="capture-input"
          autoFocus
          rows={5}
          value={text}
          disabled={working}
          placeholder="Say what's on your mind…"
          onChange={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              saveTyped();
            }
            if (e.key === "Escape") {
              e.preventDefault();
              dismiss();
            }
          }}
        />
      </div>

      {error && <p className="dim">Couldn't keep that: {error}</p>}

      <div className="capture-actions">
        {canSpeak && (
          <span className={`btn${working ? " disabled" : ""}`} onClick={beginRecording}>
            [ ● Record ]
          </span>
        )}
        <span
          className={`btn btn-accent${text.trim() && !working ? "" : " disabled"}`}
          onClick={saveTyped}
        >
          [ {working ? "Keeping…" : "Keep It"} ]
        </span>
      </div>

      <KeyHints
        keys={[
          ...(desktop && canSpeak ? [{ key: shortcut, does: "record, from anywhere" }] : []),
          { key: "⌘↵", does: "keep it" },
          ...(desktop ? [{ key: "esc", does: "hide" }] : []),
        ]}
      />
    </div>
  );
}

/// One key per line. The previous version put two shortcuts in one
/// sentence and nobody could tell where the first ended.
function KeyHints({ keys }: { keys: { key: string; does: string }[] }) {
  return (
    <dl className="key-hints">
      {keys.map((hint) => (
        <div key={hint.key}>
          <dt>{hint.key}</dt>
          <dd>{hint.does}</dd>
        </div>
      ))}
    </dl>
  );
}
