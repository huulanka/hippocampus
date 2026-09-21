// Thin wrapper over the Tauri APIs the app uses.
//
// The same frontend also runs in a plain browser via `npm run dev`, where
// none of these exist. Every call is therefore guarded by `isTauri()` and
// degrades to doing nothing, so the browser stays a usable development
// target instead of throwing on load.

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appLogDir } from "@tauri-apps/api/path";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { error as writeErrorLog } from "@tauri-apps/plugin-log";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";

/// Emitted by the Rust side when the global shortcut is pressed. Must match
/// `FOCUS_EVENT` in `src-tauri/src/lib.rs`.
const FOCUS_CAPTURE_EVENT = "hippocampus://focus-capture";

/// What the Rust side falls back to, mirrored here so the browser build
/// and the first render have something honest to show. Must match
/// `DEFAULT_CAPTURE_SHORTCUT` in `src-tauri/src/settings.rs`.
export const DEFAULT_CAPTURE_SHORTCUT = "Super+Shift+KeyH";

export function runningInDesktopApp(): boolean {
  return isTauri();
}

/// Writes to the app's log file (see `tauri_plugin_log` in `lib.rs`), so a
/// failure is on disk for the Logs button to show even if nobody was
/// looking at a terminal when it happened. Falls back to the console in
/// the browser build, where there is no such file.
export function logError(message: string): void {
  if (!isTauri()) {
    console.error(message);
    return;
  }
  writeErrorLog(message).catch(() => {});
}

/// Opens the folder the app's own logs (and the backend's, once pointed at
/// the same machine) live in, so diagnosing a problem on either Mac never
/// requires a terminal.
export async function openLogDirectory(): Promise<void> {
  if (!isTauri()) return;
  const dir = await appLogDir();
  await openPath(dir);
}

/// Opens a link in the system's default browser rather than inside the
/// app's own webview — used for the GitHub link in About.
export async function openExternalLink(url: string): Promise<void> {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await openUrl(url);
}

/// Subscribes to the global-shortcut summon. Returns an unsubscribe
/// function; in the browser that function is simply a no-op.
export function onSummonCapture(handler: () => void): () => void {
  if (!isTauri()) return () => {};

  let dispose: (() => void) | undefined;
  let cancelled = false;

  listen(FOCUS_CAPTURE_EVENT, handler).then((unlisten) => {
    // The component may already have unmounted while `listen` was in
    // flight; without this the listener would leak.
    if (cancelled) unlisten();
    else dispose = unlisten;
  });

  return () => {
    cancelled = true;
    dispose?.();
  };
}

/// Sends the window away again. Used by Escape, so the capture field
/// behaves like a panel that appears and disappears rather than an app you
/// have to put down.
export async function hideWindow(): Promise<void> {
  if (!isTauri()) return;
  await getCurrentWindow().hide();
}

/// One echo as returned by the Rust side; mirrors `contracts::EchoItem`.
export interface DesktopEchoItem {
  capture_event_id: string;
  transcript_text: string;
  occurred_at: string;
  similarity: number;
  rerank_score: number | null;
}

/// Result of a spoken capture: what was heard, and what it echoed.
export interface VoiceCapture {
  transcript: string;
  duration_ms: number;
  model: string;
  capture: {
    event_id: string;
    occurred_at: string;
    echo: DesktopEchoItem[];
  };
}

/// Whether on-device speech recognition is ready. False when the model has
/// not been fetched yet, so the UI can stay honest about what it offers
/// rather than failing after the user has already spoken.
export async function speechAvailable(): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    return await invoke<boolean>("speech_available");
  } catch {
    return false;
  }
}

export function startRecording(): Promise<void> {
  return invoke("start_recording");
}

/// Stops recording, transcribes on this machine, and stores audio and
/// transcript. Takes as long as transcription does — a few hundred
/// milliseconds for a short note.
export function stopRecording(): Promise<VoiceCapture> {
  return invoke<VoiceCapture>("stop_recording");
}

/// Discards the recording without transcribing or storing anything.
export function cancelRecording(): Promise<void> {
  return invoke("cancel_recording");
}

export interface Settings {
  capture_shortcut: string;
  /// `null` means the built-in default (`http://localhost:8080`), not
  /// "no backend" — mirrors `settings::Settings::backend_url` in Rust.
  backend_url: string | null;
}

/// Reads the persisted settings. In the browser there are none, so the
/// defaults come back instead of an error.
export async function getSettings(): Promise<Settings> {
  if (!isTauri()) return { capture_shortcut: DEFAULT_CAPTURE_SHORTCUT, backend_url: null };
  return await invoke<Settings>("get_settings");
}

/// Registers a new capture shortcut. Rejects with the Rust side's message
/// when the combination is already owned by something else, in which case
/// the previous shortcut is still in force.
export function setCaptureShortcut(accelerator: string): Promise<Settings> {
  return invoke<Settings>("set_capture_shortcut", { accelerator });
}

/// Persists which backend the client talks to. Pass `null` (or an empty
/// string) to go back to the built-in default. Does not itself change
/// which backend the running app is using — callers apply the result via
/// `setApiBaseUrl` after this resolves, once they trust the value.
export function setBackendUrl(url: string | null): Promise<Settings> {
  return invoke<Settings>("set_backend_url", { url });
}

const MODIFIER_SYMBOLS: Record<string, string> = {
  control: "⌃",
  ctrl: "⌃",
  alt: "⌥",
  option: "⌥",
  shift: "⇧",
  super: "⌘",
  cmd: "⌘",
  command: "⌘",
};

/// macOS prints modifiers in this order regardless of how they were typed.
const MODIFIER_ORDER = ["⌃", "⌥", "⇧", "⌘"];

const KEY_SYMBOLS: Record<string, string> = {
  Space: "Space",
  Enter: "↵",
  Tab: "⇥",
  Backspace: "⌫",
  Escape: "⎋",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
};

function keyLabel(code: string): string {
  if (KEY_SYMBOLS[code]) return KEY_SYMBOLS[code];
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Numpad")) return `num ${code.slice(6)}`;
  return code;
}

/// Turns an accelerator ("shift+super+KeyH") into what a Mac user reads
/// on a menu ("⇧⌘H").
export function formatAccelerator(accelerator: string): string {
  const modifiers: string[] = [];
  let key = "";

  for (const token of accelerator.split("+")) {
    const symbol = MODIFIER_SYMBOLS[token.trim().toLowerCase()];
    if (symbol) {
      if (!modifiers.includes(symbol)) modifiers.push(symbol);
    } else if (token.trim()) {
      key = keyLabel(token.trim());
    }
  }

  modifiers.sort((a, b) => MODIFIER_ORDER.indexOf(a) - MODIFIER_ORDER.indexOf(b));
  return modifiers.join("") + key;
}

/// Keys that are only ever part of a combination, never the combination.
const MODIFIER_CODES = [
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "CapsLock",
];

/// Builds an accelerator from a keypress, or explains why that keypress
/// will not do.
///
/// A bare key is refused on purpose: a global shortcut without a modifier
/// would swallow that key in every other application on the machine.
export function acceleratorFromEvent(event: KeyboardEvent): { accelerator: string } | { error: string } {
  if (MODIFIER_CODES.includes(event.code)) {
    return { error: "and one more key" };
  }

  const modifiers: string[] = [];
  if (event.ctrlKey) modifiers.push("Control");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Super");

  const holding = modifiers.some((m) => m === "Control" || m === "Alt" || m === "Super");
  if (!holding) {
    return { error: "needs ⌘, ⌃ or ⌥ — otherwise it would swallow that key everywhere" };
  }

  return { accelerator: [...modifiers, event.code].join("+") };
}
