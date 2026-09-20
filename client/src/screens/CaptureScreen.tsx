import { useEffect, useState } from "react";
import { Mascot } from "../mascot";

function formatElapsed(seconds: number): string {
  const m = Math.floor(seconds / 60)
    .toString()
    .padStart(2, "0");
  const s = (seconds % 60).toString().padStart(2, "0");
  return `${m}:${s}`;
}

// UI preview only: local transcription (transcribe-rs) and the POST
// /captures call are not wired up yet — that's the next client milestone.
// This screen exists so the interaction (start → listening → done) is
// already in place once real audio capture lands.
export function CaptureScreen() {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (!recording) return;
    const id = setInterval(() => setElapsed((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, [recording]);

  if (!recording) {
    return (
      <div className="capture-idle">
        <Mascot state="idle" cell={9} />
        <h3>ready when you are</h3>
        <p className="dim">Local transcription isn't wired up yet — this starts the UI flow only.</p>
        <span
          className="btn btn-accent"
          onClick={() => {
            setElapsed(0);
            setRecording(true);
          }}
        >
          [ Start Recording ]
        </span>
      </div>
    );
  }

  return (
    <div className="capture-idle">
      <Mascot state="listening" cell={9} />
      <h3>listening…</h3>
      <p className="dim">{formatElapsed(elapsed)}</p>
      <div className="panel transcript-panel">
        <div className="kicker">
          [ LIVE TRANSCRIPT ] <span className="dim">— raw, verbatim, never edited</span>
        </div>
        <p className="transcript-placeholder">
          (local transcription not connected yet<span className="blink-cursor">▌</span>)
        </p>
      </div>
      <div className="capture-actions">
        <span className="btn" onClick={() => setRecording(false)}>
          [ Pause ]
        </span>
        <span className="btn btn-accent-outline" onClick={() => setRecording(false)}>
          [ Done ]
        </span>
      </div>
    </div>
  );
}
