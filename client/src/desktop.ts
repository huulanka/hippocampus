// Thin wrapper over the Tauri APIs the app uses.
//
// The same frontend also runs in a plain browser via `npm run dev`, where
// none of these exist. Every call is therefore guarded by `isTauri()` and
// degrades to doing nothing, so the browser stays a usable development
// target instead of throwing on load.

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

/// Emitted by the Rust side when the global shortcut is pressed. Must match
/// `FOCUS_EVENT` in `src-tauri/src/lib.rs`.
const FOCUS_CAPTURE_EVENT = "hippocampus://focus-capture";

/// Human-readable form of the shortcut registered in `src-tauri/src/lib.rs`,
/// for display in the UI.
export const CAPTURE_SHORTCUT_LABEL = "⌘⇧H";

export function runningInDesktopApp(): boolean {
  return isTauri();
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
