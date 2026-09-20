import { useEffect, useRef, useState } from "react";
import { Mascot } from "../mascot";
import { createCapture, type EchoItem } from "../api";
import { CAPTURE_SHORTCUT_LABEL, hideWindow, runningInDesktopApp } from "../desktop";

/// Identifies where a capture came from. Recorded verbatim on the event as
/// its `source`, so later captures from the phone or a recorder stay
/// distinguishable from these.
const DEVICE = "mac-desktop";

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

type Saved = { transcript: string; echo: EchoItem[] };

/// Capture is text-first on purpose: it makes the whole loop — record,
/// store, echo — usable today, and macOS dictation (fn fn) already turns it
/// into a voice path. Real audio recording with on-device transcription and
/// a global hotkey lands next, and will reuse everything below the input.
export function CaptureScreen({ summons = 0 }: { summons?: number }) {
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState<Saved | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // The global shortcut should always land on an empty field, whatever was
  // on screen before. Skipped on first mount, where autoFocus already does
  // the job.
  useEffect(() => {
    if (summons === 0) return;
    setSaved(null);
    setError(null);
    setText("");
    inputRef.current?.focus();
  }, [summons]);

  /// Escape sends the window away, so the capture field behaves like a
  /// panel rather than an app you have to close. Unsaved text is kept in
  /// state, so summoning it again brings the half-finished thought back.
  function dismiss() {
    void hideWindow();
  }

  async function save() {
    const transcript = text.trim();
    if (!transcript || saving) return;

    setSaving(true);
    setError(null);
    try {
      const accepted = await createCapture(transcript, DEVICE);
      setSaved({ transcript, echo: accepted.echo });
      setText("");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  function startNext() {
    setSaved(null);
    setError(null);
    inputRef.current?.focus();
  }

  if (saved) {
    return (
      <div className="capture-idle">
        <Mascot state="idle" cell={9} />
        <h3>kept, word for word</h3>
        <div className="panel transcript-panel">
          <div className="kicker">
            [ CAPTURED ] <span className="dim">— raw, verbatim, never edited</span>
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

  return (
    <div className="capture-idle">
      <Mascot state={saving ? "thinking" : "idle"} cell={9} />
      <h3>{saving ? "keeping it…" : "ready when you are"}</h3>
      <p className="dim">
        Press <strong>fn</strong> twice for macOS dictation, then <strong>⌘↵</strong> to keep it.
        {runningInDesktopApp() && (
          <>
            {" "}
            <strong>{CAPTURE_SHORTCUT_LABEL}</strong> summons this from anywhere,{" "}
            <strong>esc</strong> sends it away.
          </>
        )}
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
          disabled={saving}
          placeholder="Say what's on your mind…"
          onChange={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              save();
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
        <span
          className={`btn btn-accent${text.trim() && !saving ? "" : " disabled"}`}
          onClick={save}
        >
          [ {saving ? "Keeping…" : "Keep It"} ]
        </span>
      </div>
    </div>
  );
}
