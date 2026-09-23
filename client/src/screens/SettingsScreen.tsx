import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useTheme } from "../theme";
import {
  DEFAULT_CAPTURE_SHORTCUT,
  acceleratorFromEvent,
  autostartEnabled,
  checkBackend,
  formatAccelerator,
  getSettings,
  openExternalLink,
  openLogDirectory,
  runningInDesktopApp,
  setAutostartEnabled,
  setBackendUrl,
  setCaptureShortcut,
  setCfAccessCredentials,
  setLockEnabled,
  setLockIdleSeconds,
  speechAvailable,
  type LockStatus,
} from "../desktop";
import { getApiBaseUrl, getBackendVersion, setApiBaseUrl } from "../api";

const GITHUB_URL = "https://github.com/huulanka/hippocampus";

/// How long the app may sit unattended, in minutes. A short list rather
/// than a free number: the difference between six and seven minutes is
/// not a decision anybody has, and every option here is one the Rust side
/// will accept unchanged.
const IDLE_CHOICES = [1, 5, 15, 60];

/// This screen used to offer a cloud transcription mode, a storage path,
/// and a "delete audio after transcription" switch that were all mocks —
/// two of them contradicted decisions the project has already made. A
/// setting that does nothing is worse than a missing one: it invites you
/// to believe something about the system that is not true. The tray icon
/// and its autostart toggle, added later, are the real thing.
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

  const [lock, setLock] = useState<LockStatus | null>(null);
  const [lockError, setLockError] = useState<string | null>(null);

  const [autostart, setAutostart] = useState(false);
  const [autostartError, setAutostartError] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then((settings) => {
      setShortcut(settings.capture_shortcut);
      setBackendUrlInput(settings.backend_url ?? getApiBaseUrl());
      setCfClientIdInput(settings.cf_access_client_id ?? "");
      setCfConfigured(settings.cf_access_configured);
      setLock(settings.lock);
    });
    speechAvailable().then(setCanSpeak);
    autostartEnabled().then(setAutostart);
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
    <div className="column settings">
      <h1 className="settings-title">Settings</h1>

      <Section title="Appearance">
        <Row label="Theme">
          <div className="segmented" role="group" aria-label="Theme">
            <button type="button" aria-pressed={theme === "dark"} onClick={() => setTheme("dark")}>
              Dark
            </button>
            <button type="button" aria-pressed={theme === "light"} onClick={() => setTheme("light")}>
              Light
            </button>
          </div>
        </Row>
      </Section>

      <Section
        title="Capture"
        note={
          runningInDesktopApp()
            ? "The shortcut summons the capture sheet from wherever you are, and takes effect immediately. Hippocampus lives in the menu bar: closing the window puts it away rather than quitting, and clicking the brain brings it back. “Quit Hippocampus” in its right-click menu is the only way out."
            : "The global shortcut and the menu bar belong to the desktop app — this is the browser build."
        }
      >
        <Row label="Global shortcut">
          {capturing ? (
            <div
              ref={captureRef}
              className="hotkey hotkey-listening"
              tabIndex={0}
              onKeyDown={onCaptureKey}
              onBlur={endCapture}
            >
              {hint}
            </div>
          ) : (
            <div className="hotkey">{formatAccelerator(shortcut)}</div>
          )}
          <button type="button" className="btn btn-secondary" onClick={capturing ? endCapture : beginCapture}>
            {capturing ? "Cancel" : "Change"}
          </button>
        </Row>
        {runningInDesktopApp() && (
          <Row label="Start at login">
            <label className="check">
              <input
                type="checkbox"
                checked={autostart}
                onChange={() => {
                  setAutostartError(null);
                  const next = !autostart;
                  setAutostartEnabled(next)
                    .then(setAutostart)
                    .catch((err) => setAutostartError(String(err)));
                }}
              />
              <span className="sr-only">Start Hippocampus at login</span>
            </label>
          </Row>
        )}
        {error && <Problem>{error}</Problem>}
        {autostartError && <Problem>{autostartError}</Problem>}
      </Section>

      <Section
        title="Backend"
        note={
          backendStatus === "saved"
            ? "Saved — checked reachable just now."
            : backendStatus === "error"
              ? undefined
              : "Where captures go. Checked against /health before it is saved, so a typo cannot strand this screen."
        }
      >
        <Row label="Address" htmlFor="settings-backend">
          <input
            id="settings-backend"
            className="input settings-input"
            type="text"
            value={backendUrlInput}
            placeholder="http://localhost:8080"
            onChange={(e) => {
              setBackendUrlInput(e.target.value);
              setBackendStatus("idle");
            }}
          />
          <button
            type="button"
            className="btn btn-secondary"
            disabled={backendStatus === "checking"}
            onClick={saveBackendUrl}
          >
            {backendStatus === "checking" ? "Checking…" : "Save"}
          </button>
        </Row>
        {backendStatus === "error" && <Problem>Not saved: {backendError}</Problem>}
      </Section>

      <Section
        title="Cloudflare Access"
        note={
          cfStatus === "saved"
            ? "Saved — the secret is in the macOS Keychain."
            : cfStatus === "error"
              ? undefined
              : cfConfigured
                ? "A secret is saved in the macOS Keychain. Leave the field empty to keep it, type a new one to replace it, or clear both and save to remove it."
                : "Only needed once the backend sits behind Cloudflare Access — a Zero Trust Service Token, not your own login. Leave both blank on a local or LAN backend. The secret goes to the Keychain, never to a file."
        }
      >
        <Row label="Client ID" htmlFor="settings-cf-id">
          <input
            id="settings-cf-id"
            className="input settings-input"
            type="text"
            value={cfClientIdInput}
            placeholder="Blank on a local backend"
            onChange={(e) => {
              setCfClientIdInput(e.target.value);
              setCfStatus("idle");
            }}
          />
        </Row>
        <Row label="Client secret" htmlFor="settings-cf-secret">
          <input
            id="settings-cf-secret"
            className="input settings-input"
            type="password"
            value={cfClientSecretInput}
            placeholder={cfConfigured ? "Saved in the Keychain" : "Blank on a local backend"}
            onChange={(e) => {
              setCfClientSecretInput(e.target.value);
              setCfStatus("idle");
            }}
          />
        </Row>
        {/* One button for both fields, under both: it saves the pair,
            and sitting in the secret's row it made that one field
            narrower than the one above it. */}
        <div className="settings-row settings-row-action">
          <button
            type="button"
            className="btn btn-secondary"
            disabled={cfStatus === "checking"}
            onClick={saveCfAccessCredentials}
          >
            {cfStatus === "checking" ? "Checking…" : "Save both"}
          </button>
        </div>
        {cfStatus === "error" && <Problem>Not saved: {cfError}</Problem>}
      </Section>

      <Section
        title="Reading"
        note={
          lock === null || lock.mechanism === "none"
            ? runningInDesktopApp()
              ? "This Mac has no device authentication set up, so there is nothing to lock with. Turn on Touch ID or a login password in System Settings and this becomes available — until then the app deliberately stays open rather than shutting you out of your own notes."
              : "Only the desktop app can lock — this is the browser build."
            : "Reading is what is guarded: the timeline, search, entities, the graph, a capture and its recording. Capturing is not — speaking a note only ever adds, and a prompt in front of the shortcut would cost the fastest thing the app does. The clock only runs while the window is not in front, so nothing disappears while you are reading it."
        }
      >
        {lock !== null && lock.mechanism !== "none" && (
          <>
            <Row label={`Ask for ${lock.mechanism === "touchid" ? "Touch ID" : "your password"}`}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={lock.enabled}
                  onChange={() => {
                    setLockError(null);
                    setLockEnabled(!lock.enabled)
                      .then((settings) => setLock(settings.lock))
                      .catch((err) => setLockError(String(err)));
                  }}
                />
                <span className="sr-only">Ask before reading</span>
              </label>
            </Row>
            {lock.enabled && (
              <Row label="Locks again after">
                <div className="settings-choices">
                  {IDLE_CHOICES.map((minutes) => (
                    <button
                      key={minutes}
                      type="button"
                      className="chip"
                      aria-pressed={lock.idle_seconds === minutes * 60}
                      onClick={() => {
                        setLockError(null);
                        setLockIdleSeconds(minutes * 60)
                          .then((settings) => setLock(settings.lock))
                          .catch((err) => setLockError(String(err)));
                      }}
                    >
                      {minutes} min
                    </button>
                  ))}
                </div>
              </Row>
            )}
            {lockError && <Problem>{lockError}</Problem>}
          </>
        )}
      </Section>

      <Section
        title="Speech"
        note={
          canSpeak
            ? "On-device, always. Your voice is the most revealing thing this system holds, so the audio never leaves this Mac — only the transcript is sent on."
            : "The speech model is not installed. Run scripts/fetch-asr-model.sh to enable spoken capture; typing works either way."
        }
      />

      <Section
        title="What is kept"
        note="Recordings are kept for good, alongside their transcripts — the recording is the original, the transcript is one reading of it. Nothing here deletes anything yet."
      />

      <Section
        title="Logs"
        note={
          runningInDesktopApp()
            ? "Everything this app writes down about itself, in case something needs debugging without a terminal."
            : "Only the desktop app keeps a log file — this is the browser build."
        }
      >
        <Row label="On this Mac">
          <button
            type="button"
            className="btn btn-secondary"
            disabled={!runningInDesktopApp()}
            onClick={openLogDirectory}
          >
            Open log folder
          </button>
        </Row>
      </Section>

      <Section title="About">
        <Row label="Versions">
          <span className="meta">
            {runningInDesktopApp() ? `App ${clientVersion ?? "…"}` : "Browser build"}
            {" · "}
            {backendVersion ? `Backend ${backendVersion}` : "backend unreachable"}
          </span>
        </Row>
        <Row label="Source">
          <button type="button" className="btn btn-secondary" onClick={() => openExternalLink(GITHUB_URL)}>
            GitHub
          </button>
        </Row>
      </Section>
    </div>
  );
}

/// One group of settings: a name, its rows, and one explanation underneath.
///
/// The note goes at the bottom rather than between the heading and the
/// controls — the heading says what this is, the controls are what you came
/// for, and the prose is for the first time you read it.
function Section({
  title,
  note,
  children,
}: {
  title: string;
  note?: string;
  children?: React.ReactNode;
}) {
  return (
    <section className="settings-section">
      <h2 className="label-micro">{title}</h2>
      {children && <div className="settings-rows">{children}</div>}
      {note && <p className="settings-note">{note}</p>}
    </section>
  );
}

/// One setting: what it is on the left, what you do about it on the right.
/// With `htmlFor` the name is a real label, so clicking it focuses the field
/// and a screen reader announces what the field is for.
function Row({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="settings-row">
      {htmlFor ? (
        <label className="settings-label" htmlFor={htmlFor}>
          {label}
        </label>
      ) : (
        <span className="settings-label">{label}</span>
      )}
      <span className="settings-control">{children}</span>
    </div>
  );
}

function Problem({ children }: { children: React.ReactNode }) {
  return <p className="settings-problem">{children}</p>;
}
