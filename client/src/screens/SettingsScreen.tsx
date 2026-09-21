import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useTheme } from "../theme";
import {
  DEFAULT_CAPTURE_SHORTCUT,
  acceleratorFromEvent,
  checkBackend,
  formatAccelerator,
  getSettings,
  openExternalLink,
  openLogDirectory,
  runningInDesktopApp,
  setBackendUrl,
  setCaptureShortcut,
  setCfAccessCredentials,
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

  const [cfClientIdInput, setCfClientIdInput] = useState("");
  const [cfClientSecretInput, setCfClientSecretInput] = useState("");
  /// Whether a secret is in the Keychain. The secret itself never comes
  /// here — the field below stays empty even when one is stored, and an
  /// empty field on save means "keep the stored one".
  const [cfConfigured, setCfConfigured] = useState(false);
  const [cfStatus, setCfStatus] = useState<"idle" | "checking" | "saved" | "error">("idle");
  const [cfError, setCfError] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then((settings) => {
      setShortcut(settings.capture_shortcut);
      setBackendUrlInput(settings.backend_url ?? getApiBaseUrl());
      setCfClientIdInput(settings.cf_access_client_id ?? "");
      setCfConfigured(settings.cf_access_configured);
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
  ///
  /// The check runs in Rust, not as a `fetch` from here. A browser
  /// request carrying the Access headers needs a CORS preflight first,
  /// and an `OPTIONS` with no credentials is exactly what Cloudflare
  /// Access answers with a login redirect — which the webview can only
  /// report as `TypeError: Load failed`, with no way to tell a wrong URL
  /// from a wrong token from a backend that is simply down.
  ///
  /// Checked with whatever credentials are currently in the other
  /// section's fields, saved or not — a backend that was already behind
  /// Access before this screen loaded has no working order to save these
  /// independently in otherwise: the URL check would always fail Access
  /// with no token attached, and the token check would always be
  /// validated against the wrong (previous) backend.
  async function saveBackendUrl() {
    const wanted = backendUrlInput.trim();
    setBackendStatus("checking");
    setBackendError(null);

    try {
      await checkBackend(
        wanted || null,
        cfClientIdInput.trim() || null,
        cfClientSecretInput.trim() || null,
      );
    } catch (err) {
      setBackendStatus("error");
      setBackendError(String(err));
      return;
    }

    await setBackendUrl(wanted || null);
    setApiBaseUrl(wanted);
    setBackendUrlInput(getApiBaseUrl());
    setBackendStatus("saved");
  }

  /// Same "prove it works before committing" shape as `saveBackendUrl`,
  /// checked with the new credentials actually attached — a wrong secret
  /// should surface here, not as a mysteriously blocked capture later.
  ///
  /// An empty secret field with a Client ID present means "keep the one
  /// in the Keychain", which is how this screen can show and change the
  /// ID without ever holding the secret that belongs to it.
  async function saveCfAccessCredentials() {
    const id = cfClientIdInput.trim();
    const secret = cfClientSecretInput.trim();

    if (!id && !secret) {
      try {
        await setCfAccessCredentials(null, null);
      } catch (err) {
        setCfStatus("error");
        setCfError(String(err));
        return;
      }
      setCfConfigured(false);
      setCfStatus("saved");
      setCfError(null);
      return;
    }
    if (!id) {
      setCfStatus("error");
      setCfError("need a Client ID as well — a secret on its own is not a token");
      return;
    }
    if (!secret && !cfConfigured) {
      setCfStatus("error");
      setCfError("need both Client ID and Client Secret, or neither");
      return;
    }

    setCfStatus("checking");
    setCfError(null);

    try {
      await checkBackend(backendUrlInput.trim() || null, id, secret || null);
    } catch (err) {
      setCfStatus("error");
      setCfError(String(err));
      return;
    }

    try {
      await setCfAccessCredentials(id, secret || null);
    } catch (err) {
      // Reaching here means the Keychain refused. Saying "saved" would
      // be a lie the user only discovers on the next restart.
      setCfStatus("error");
      setCfError(String(err));
      return;
    }
    setCfClientSecretInput("");
    setCfConfigured(true);
    setCfStatus("saved");
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

      <h4 className="section-label">CLOUDFLARE ACCESS</h4>
      <div className="settings-row">
        <input
          className="hotkey-display settings-input"
          type="text"
          value={cfClientIdInput}
          placeholder="Client ID"
          onChange={(e) => {
            setCfClientIdInput(e.target.value);
            setCfStatus("idle");
          }}
        />
      </div>
      <div className="settings-row">
        <input
          className="hotkey-display settings-input"
          type="password"
          value={cfClientSecretInput}
          placeholder={cfConfigured ? "Client Secret — saved in the Keychain" : "Client Secret"}
          onChange={(e) => {
            setCfClientSecretInput(e.target.value);
            setCfStatus("idle");
          }}
        />
        <span
          className="btn"
          onClick={cfStatus === "checking" ? undefined : saveCfAccessCredentials}
        >
          [ {cfStatus === "checking" ? "Checking…" : "Save"} ]
        </span>
      </div>
      <p className="dim settings-note">
        {cfStatus === "saved" && "Saved — the secret is in the macOS Keychain."}
        {cfStatus === "error" && `Not saved: ${cfError}`}
        {cfStatus === "idle" &&
          (cfConfigured
            ? "A secret is saved in the macOS Keychain. Leave the field empty to keep it; type a new one to replace it; clear both fields and save to remove it."
            : "Only needed once the backend sits behind Cloudflare Access — a Zero Trust Service Token, not your own login. Leave both blank on a local or LAN backend. The secret goes to the macOS Keychain, never to a file.")}
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
