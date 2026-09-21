import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useTheme } from "../theme";
import {
  DEFAULT_CAPTURE_SHORTCUT,
  acceleratorFromEvent,
  formatAccelerator,
  getSettings,
  openExternalLink,
  openLogDirectory,
  runningInDesktopApp,
  setBackendUrl,
  setCaptureShortcut,
  speechAvailable,
} from "../desktop";
import { getApiBaseUrl, getBackendVersion, setApiBaseUrl } from "../api";

const GITHUB_URL = "https://github.com/huulanka/hippocampus";

/// Only the hotkey, the backend URL and the theme are real so far.
/// Everything else this screen used to offer — a tray icon, a cloud
/// transcription mode, a storage path, a "delete audio after
/// transcription" switch — was a mock, and two of those switches
/// contradicted decisions the project has already made. A setting that
/// does nothing is worse than a missing one: it invites you to believe
/// something about the system that is not true.
export function SettingsScreen() {
  const { theme, setTheme } = useTheme();
  const [shortcut, setShortcut] = useState(DEFAULT_CAPTURE_SHORTCUT);
  const [capturing, setCapturing] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [canSpeak, setCanSpeak] = useState(false);
  const captureRef = useRef<HTMLDivElement>(null);

  const [backendUrlInput, setBackendUrlInput] = useState(getApiBaseUrl());
  const [backendStatus, setBackendStatus] = useState<"idle" | "checking" | "saved" | "error">(
    "idle",
  );
  const [backendError, setBackendError] = useState<string | null>(null);

  const [clientVersion, setClientVersion] = useState<string | null>(null);
  const [backendVersion, setBackendVersion] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then((settings) => {
      setShortcut(settings.capture_shortcut);
      setBackendUrlInput(settings.backend_url ?? getApiBaseUrl());
    });
    speechAvailable().then(setCanSpeak);
    if (runningInDesktopApp()) getVersion().then(setClientVersion);
    getBackendVersion()
      .then(setBackendVersion)
      .catch(() => setBackendVersion(null));
  }, []);

  /// Only committed once `/health` actually answers — a typo here would
  /// otherwise strand every other screen against a backend that cannot be
  /// reached, including this one.
  async function saveBackendUrl() {
    const wanted = backendUrlInput.trim();
    setBackendStatus("checking");
    setBackendError(null);

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);
    try {
      const res = await fetch(`${wanted || getApiBaseUrl()}/health`, {
        signal: controller.signal,
      });
      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
    } catch (err) {
      setBackendStatus("error");
      setBackendError(
        err instanceof Error && err.name === "AbortError"
          ? "no answer within 5s"
          : String(err),
      );
      return;
    } finally {
      clearTimeout(timeout);
    }

    await setBackendUrl(wanted || null);
    setApiBaseUrl(wanted);
    setBackendUrlInput(getApiBaseUrl());
    setBackendStatus("saved");
  }

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

      <h4 className="section-label">BACKEND</h4>
      <div className="settings-row">
        <input
          className="hotkey-display settings-input"
          type="text"
          value={backendUrlInput}
          placeholder="http://localhost:8080"
          onChange={(e) => {
            setBackendUrlInput(e.target.value);
            setBackendStatus("idle");
          }}
        />
        <span
          className="btn"
          onClick={backendStatus === "checking" ? undefined : saveBackendUrl}
        >
          [ {backendStatus === "checking" ? "Checking…" : "Save"} ]
        </span>
      </div>
      <p className="dim settings-note">
        {backendStatus === "saved" && "Saved — checked reachable just now."}
        {backendStatus === "error" && `Not saved: ${backendError}`}
        {backendStatus === "idle" &&
          "Where captures go. Checked against /health before it is saved, so a typo cannot strand this screen."}
      </p>

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

      <h4 className="section-label">LOGS</h4>
      <div className="settings-row">
        <span className="btn" onClick={runningInDesktopApp() ? openLogDirectory : undefined}>
          [ Open Log Folder ]
        </span>
      </div>
      <p className="dim settings-note">
        {runningInDesktopApp()
          ? "Everything this app writes down about itself, in case something needs debugging without a terminal."
          : "Only the desktop app keeps a log file — this is the browser build."}
      </p>

      <h4 className="section-label">ABOUT</h4>
      <p className="dim settings-note">
        {runningInDesktopApp() ? `App v${clientVersion ?? "…"}` : "Browser build"}
        {" — "}
        {backendVersion ? `Backend v${backendVersion}` : "backend unreachable"}
      </p>
      <div className="settings-row">
        <span className="btn" onClick={() => openExternalLink(GITHUB_URL)}>
          [ GitHub ]
        </span>
      </div>
    </>
  );
}
