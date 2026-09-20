import { useEffect, useRef, useState } from "react";
import { useTheme } from "../theme";
import {
  DEFAULT_CAPTURE_SHORTCUT,
  acceleratorFromEvent,
  formatAccelerator,
  getSettings,
  runningInDesktopApp,
  setCaptureShortcut,
  speechAvailable,
} from "../desktop";

/// Only the hotkey is real so far. Everything else this screen used to
/// offer — a tray icon, a cloud transcription mode, a storage path, a
/// "delete audio after transcription" switch — was a mock, and two of
/// those switches contradicted decisions the project has already made.
/// A setting that does nothing is worse than a missing one: it invites
/// you to believe something about the system that is not true.
export function SettingsScreen() {
  const { theme, setTheme } = useTheme();
  const [shortcut, setShortcut] = useState(DEFAULT_CAPTURE_SHORTCUT);
  const [capturing, setCapturing] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [canSpeak, setCanSpeak] = useState(false);
  const captureRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    getSettings().then((settings) => setShortcut(settings.capture_shortcut));
    speechAvailable().then(setCanSpeak);
  }, []);

  useEffect(() => {
    if (capturing) captureRef.current?.focus();
  }, [capturing]);

  function beginCapture() {
    setError(null);
    setHint("press the combination you want");
    setCapturing(true);
  }

  function endCapture() {
    setCapturing(false);
    setHint(null);
  }

  async function onCaptureKey(event: React.KeyboardEvent) {
    event.preventDefault();

    if (event.key === "Escape") {
      endCapture();
      return;
    }

    const result = acceleratorFromEvent(event.nativeEvent);
    if ("error" in result) {
      setHint(result.error);
      return;
    }

    try {
      const settings = await setCaptureShortcut(result.accelerator);
      setShortcut(settings.capture_shortcut);
      endCapture();
    } catch (err) {
      setError(String(err));
      endCapture();
    }
  }

  return (
    <>
      <h4 className="section-label">APPEARANCE</h4>
      <div className="theme-switch">
        <span
          className={`btn${theme === "dark" ? " btn-accent" : ""}`}
          onClick={() => setTheme("dark")}
        >
          [ Dark ]
        </span>
        <span
          className={`btn${theme === "light" ? " btn-accent" : ""}`}
          onClick={() => setTheme("light")}
        >
          [ Light ]
        </span>
      </div>

      <h4 className="section-label">GLOBAL HOTKEY</h4>
      <div className="settings-row">
        {capturing ? (
          <div
            ref={captureRef}
            className="hotkey-display hotkey-capturing"
            tabIndex={0}
            onKeyDown={onCaptureKey}
            onBlur={endCapture}
          >
            {hint}
          </div>
        ) : (
          <div className="hotkey-display">{formatAccelerator(shortcut)}</div>
        )}
        <span className="btn" onClick={capturing ? endCapture : beginCapture}>
          [ {capturing ? "Cancel" : "Change"} ]
        </span>
      </div>
      <p className="dim settings-note">
        {runningInDesktopApp()
          ? "Summons the capture field from wherever you are. Takes effect immediately; esc cancels."
          : "Only the desktop app has a global hotkey — this is the browser build."}
      </p>
      {error && <p className="dim settings-note">{error}</p>}

      <h4 className="section-label">SPEECH RECOGNITION</h4>
      <p className="dim settings-note">
        {canSpeak
          ? "On-device, always. Your voice is the most revealing thing this system holds, so the audio never leaves this Mac — only the transcript is sent on."
          : "The speech model is not installed. Run scripts/fetch-asr-model.sh to enable spoken capture; typing works either way."}
      </p>

      <h4 className="section-label">WHAT IS KEPT</h4>
      <p className="dim settings-note">
        Recordings are kept for good, alongside their transcripts — the recording is the
        original, the transcript is one reading of it. Nothing here deletes anything yet.
      </p>
    </>
  );
}
