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

/// The same app, on an iPhone. `runningInDesktopApp` is true there as
/// well — it is the Tauri shell either way — but the shortcut, the menu
/// bar, the login item, the calendar and the log folder are Mac things,
/// and the Rust side does not register their commands on iOS at all.
///
/// Asked two ways, because either alone has failed somewhere: the user
/// agent names the device, and a touch screen is the one thing no Mac has.
export function runningOnPhone(): boolean {
  return isTauri() && (/iPhone|iPad|iPod/.test(navigator.userAgent) || navigator.maxTouchPoints > 1);
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

/// Result of a capture: what was heard, and — when the backend was
/// reachable — what it echoed.
export interface VoiceCapture {
  transcript: string;
  duration_ms: number;
  model: string;
  /// `null` when the backend could not be reached. The capture is on this
  /// Mac either way; this only says whether it has arrived yet, so there
  /// is no echo and no id to open.
  capture: {
    event_id: string;
    occurred_at: string;
    /// Empty on arrival — the echo is judged after the capture is stored,
    /// so that speaking a note is confirmed as saved immediately.
    echo: DesktopEchoItem[];
    echo_pending: boolean;
  } | null;
  /// Why it has not arrived yet, when it has not.
  queued_reason: string | null;
}

/// How much is waiting on this Mac to reach the backend. Mirrors
/// `outbox::Counts` in Rust.
export interface OutboxCounts {
  waiting: number;
  /// How many of those have already failed at least once. One waiting
  /// capture is a moment; one that has failed eleven times is a problem,
  /// and the sidebar says so differently.
  failing: number;
  last_error: string | null;
}

/// Emitted by the Rust side whenever the queue changes. Must match
/// `OUTBOX_EVENT` in `src-tauri/src/sync.rs`.
const OUTBOX_EVENT = "hippocampus://outbox";

export async function outboxStatus(): Promise<OutboxCounts> {
  if (!isTauri()) return { waiting: 0, failing: 0, last_error: null };
  try {
    return await invoke<OutboxCounts>("outbox_status");
  } catch {
    return { waiting: 0, failing: 0, last_error: null };
  }
}

/// Retries the whole queue now, because someone asked rather than because
/// a timer fired.
export async function syncNow(): Promise<OutboxCounts> {
  if (!isTauri()) return { waiting: 0, failing: 0, last_error: null };
  return await invoke<OutboxCounts>("sync_now");
}

/// Subscribes to changes in the queue. Same shape as `onSummonCapture`:
/// returns an unsubscribe function, a no-op in the browser build.
export function onOutboxChange(handler: (counts: OutboxCounts) => void): () => void {
  if (!isTauri()) return () => {};

  let dispose: (() => void) | undefined;
  let cancelled = false;

  listen<OutboxCounts>(OUTBOX_EVENT, (event) => handler(event.payload)).then((unlisten) => {
    if (cancelled) unlisten();
    else dispose = unlisten;
  });

  return () => {
    cancelled = true;
    dispose?.();
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

/// Stores a typed capture the same way a spoken one is stored: on this
/// Mac first, then sent. Returns `null` in the browser build, where there
/// is no outbox and `api.ts` posts directly instead.
export async function captureText(transcript: string): Promise<VoiceCapture | null> {
  if (!isTauri()) return null;
  return await invoke<VoiceCapture>("capture_text", { transcript });
}

/// Discards the recording without transcribing or storing anything.
export function cancelRecording(): Promise<void> {
  return invoke("cancel_recording");
}

export interface Settings {
  capture_shortcut: string;
  /// `null` means the built-in default (`http://localhost:8080`), not
  /// "no backend" — mirrors `settings::Stored::backend_url` in Rust.
  backend_url: string | null;
  /// The Cloudflare Access Service Token's Client ID. An identifier, not
  /// a credential — the Client Secret lives in the macOS Keychain and is
  /// deliberately never sent to this side.
  cf_access_client_id: string | null;
  /// Whether a Client Secret is stored. All the webview needs in order to
  /// render "a secret is saved" and offer to replace it.
  cf_access_configured: boolean;
  lock: LockStatus;
  review: ReviewSchedule;
  foresight: ForesightSettings;
}

/// How meetings are brought up on this Mac. Mirrors
/// `settings::ForesightSettings`.
export interface ForesightSettings {
  /// EventKit identifiers of the calendars *this* Mac reads. Empty means
  /// none — nothing is read until something is ticked.
  calendar_ids: string[];
  lead_minutes: number;
  banner: boolean;
}

export const FORESIGHT_LEADS = [5, 10, 15, 30] as const;

const DEFAULT_FORESIGHT: ForesightSettings = { calendar_ids: [], lead_minutes: 10, banner: true };

/// Whether the calendar may be read, as macOS sees it.
export type CalendarAccess = "not_determined" | "denied" | "restricted" | "granted" | "write_only";

export interface CalendarInfo {
  id: string;
  title: string;
  /// The account — "iCloud", "Exchange", an address.
  account: string;
  color: string | null;
}

/// Where a meeting stands, seen from now.
export type MeetingPhase = "ahead" | "now" | "after";

/// One meeting worth showing. Mirrors `foresight::MeetingView`: the
/// meeting and ids only — the words about it are fetched through the
/// gated request path by whoever shows them.
export interface MeetingView {
  key: string;
  title: string;
  starts_at: string;
  ends_at: string;
  people: string[];
  phase: MeetingPhase;
  known: number;
  intentions: string[];
  answered: boolean;
}

export interface ForesightStatus {
  access: CalendarAccess;
  watching: number;
  meetings: MeetingView[];
  /// The meeting the menu bar is lit for.
  attention: string | null;
  error: string | null;
}

const NO_FORESIGHT: ForesightStatus = {
  access: "restricted",
  watching: 0,
  meetings: [],
  attention: null,
  error: null,
};

/// When the weekly review is announced. Mirrors `settings::ReviewSchedule`.
export interface ReviewSchedule {
  enabled: boolean;
  /// 0 = Monday … 6 = Sunday.
  weekday: number;
  /// Local time, "HH:MM".
  time: string;
}

/// Friday at four — the Rust side's default, for the browser build.
const DEFAULT_REVIEW: ReviewSchedule = { enabled: false, weekday: 4, time: "16:00" };

/// The guard in front of the notes. Mirrors `lock::LockStatus` in Rust.
///
/// Note what this is *not*: it is not what enforces anything. The Rust
/// side refuses the requests, and this is only enough to draw the right
/// screen — so a webview that lied to itself about it would see a lock
/// screen missing and every read still refused.
export interface LockStatus {
  /// What this machine can actually authenticate with. `"none"` means it
  /// cannot, and the guard makes itself inert rather than shutting the
  /// owner out of their own notes.
  mechanism: "touchid" | "password" | "none";
  enabled: boolean;
  locked: boolean;
  idle_seconds: number;
}

/// What the guard looks like where there is no Rust side to ask — the
/// browser build, which has no device authentication to offer.
const NO_LOCK: LockStatus = {
  mechanism: "none",
  enabled: false,
  locked: false,
  idle_seconds: 0,
};

/// Reads the persisted settings. In the browser there are none, so the
/// defaults come back instead of an error.
export async function getSettings(): Promise<Settings> {
  if (!isTauri())
    return {
      capture_shortcut: DEFAULT_CAPTURE_SHORTCUT,
      backend_url: null,
      cf_access_client_id: null,
      cf_access_configured: false,
      lock: NO_LOCK,
      // Off: the browser build has no way to announce anything.
      review: DEFAULT_REVIEW,
      foresight: DEFAULT_FORESIGHT,
    };
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

/// Persists the Cloudflare Access Service Token: the ID in the settings
/// file, the secret in the Keychain.
///
/// A `null` Client ID clears both. A `null` Client Secret with an ID
/// present keeps the secret already stored, which is how the settings
/// screen can save a changed ID without ever having held the secret.
/// Rejects when the Keychain refuses — a credential the user believes is
/// saved and is not would surface much later as an unexplained 403.
export function setCfAccessCredentials(
  clientId: string | null,
  clientSecret: string | null,
): Promise<Settings> {
  return invoke<Settings>("set_cf_access_credentials", {
    clientId,
    clientSecret,
  });
}

/// The state of the guard, asked for directly rather than through the
/// whole settings object — this is polled on navigation, and reading the
/// Keychain for a Client Secret on the way would be an odd side effect of
/// asking whether the screen is locked.
export function lockStatus(): Promise<LockStatus> {
  if (!isTauri()) return Promise.resolve(NO_LOCK);
  return invoke<LockStatus>("lock_status");
}

/// Puts up the system's own authentication dialog and resolves with the
/// state afterwards — `locked: false` when it was answered, still `true`
/// when it was cancelled. Rejects only when the dialog itself failed.
export function unlock(): Promise<LockStatus> {
  if (!isTauri()) return Promise.resolve(NO_LOCK);
  return invoke<LockStatus>("unlock");
}

/// Closes the guard by hand, for leaving the desk without waiting out the
/// idle timer.
export function lockNow(): Promise<LockStatus> {
  if (!isTauri()) return Promise.resolve(NO_LOCK);
  return invoke<LockStatus>("lock_now");
}

/// Switches the guard off, or back on. Switching it *off* puts the
/// authentication dialog up first and rejects if it is not answered —
/// otherwise the lock screen would carry a button that removes the lock.
export function setLockEnabled(enabled: boolean): Promise<Settings> {
  return invoke<Settings>("set_lock_enabled", { enabled });
}

export function setLockIdleSeconds(seconds: number): Promise<Settings> {
  return invoke<Settings>("set_lock_idle_seconds", { seconds });
}

/// Whether the app is registered to start at login. Read straight from
/// the OS launch-agent registration, not from `settings.json` — there is
/// no separate copy of this to fall out of sync. `false` in the browser
/// build, where there is no OS to register with.
export function autostartEnabled(): Promise<boolean> {
  if (!isTauri()) return Promise.resolve(false);
  return invoke<boolean>("autostart_enabled");
}

/// Switches the login item on or off.
export function setAutostartEnabled(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_autostart_enabled", { enabled });
}

/// Fires when the app locked itself after sitting unattended. Pushed from
/// Rust rather than polled: the screen has to go away while nobody is
/// asking it anything.
export function setReviewSchedule(schedule: ReviewSchedule): Promise<Settings> {
  return invoke<Settings>("set_review_schedule", { ...schedule });
}

/// The week's review has been announced (the notification went out). The
/// app opens it the next time the window is in front.
export function onReviewDue(handler: () => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen("hippocampus://review-due", () => handler());
}

/// Open the review now — the tray's "Look Back on the Week".
export function onOpenReview(handler: () => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen("hippocampus://open-review", () => handler());
}

/// The meetings this Mac is watching, as far as they are worth showing.
/// In the browser there is no calendar, so there are none.
export async function foresightStatus(): Promise<ForesightStatus> {
  if (!isTauri()) return NO_FORESIGHT;
  return await invoke<ForesightStatus>("foresight_status");
}

export function onForesight(handler: (status: ForesightStatus) => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen<ForesightStatus>("hippocampus://foresight", (event) => handler(event.payload));
}

/// Open a meeting's brief — the lit menu bar was clicked.
export function onOpenBrief(handler: (key: string) => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen<string>("hippocampus://open-brief", (event) => handler(event.payload));
}

/// "Did you bring it up?" has been answered for this meeting.
export async function foresightAnswered(key: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("foresight_answered", { key });
}

/// Ask the backend about the meetings again now — after something was
/// ticked off, so the menu bar goes quiet without waiting for its tick.
export async function foresightRefresh(): Promise<void> {
  if (!isTauri()) return;
  await invoke("foresight_refresh");
}

export async function calendarAccess(): Promise<CalendarAccess> {
  if (!isTauri()) return "restricted";
  return await invoke<CalendarAccess>("calendar_access");
}

/// Puts up macOS's own permission dialog. Resolves with the answer.
export function requestCalendarAccess(): Promise<CalendarAccess> {
  return invoke<CalendarAccess>("request_calendar_access");
}

/// System Settings, at the calendar permission — the only way back once
/// access was refused.
export async function openCalendarPrivacy(): Promise<void> {
  if (!isTauri()) return;
  await invoke("open_calendar_privacy");
}

export async function listCalendars(): Promise<CalendarInfo[]> {
  if (!isTauri()) return [];
  return await invoke<CalendarInfo[]>("list_calendars");
}

export function setForesight(foresight: ForesightSettings): Promise<Settings> {
  return invoke<Settings>("set_foresight", {
    calendarIds: foresight.calendar_ids,
    leadMinutes: foresight.lead_minutes,
    banner: foresight.banner,
  });
}

/// Unlocked from somewhere other than this window's own gate — the
/// menu-bar menu's "Unlock…". The gate here drops without asking for
/// Touch ID a second time.
export function onUnlocked(handler: (status: LockStatus) => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen<LockStatus>("hippocampus://unlocked", (event) => handler(event.payload));
}

/// The menu's "Write a Note…": open the capture sheet for typing, without
/// the microphone.
export function onWriteNote(handler: () => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen("hippocampus://write-note", () => handler());
}

export function onLocked(handler: () => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen("hippocampus://locked", () => handler());
}

/// One HTTP exchange with the backend, performed in Rust.
export interface ApiResponse {
  status: number;
  status_text: string;
  body: string;
}

/// Sends a request through the Rust side, which knows the backend URL and
/// the Service Token. Rejects only when the request never completed; a
/// non-2xx answer comes back as a value.
export function apiRequest(
  method: string,
  path: string,
  body?: string,
): Promise<ApiResponse> {
  return invoke<ApiResponse>("api_request", { method, path, body: body ?? null });
}

/// The bytes of a capture's recording, fetched with credentials attached.
export function apiAudio(eventId: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("api_audio", { eventId });
}

/// Asks a backend whether it answers, using credentials that have not
/// been saved yet. Resolves with the normalised URL that answered.
///
/// Pass `null` for the secret to check against the one already in the
/// Keychain. Rejects with a sentence worth showing the user — including
/// the case Cloudflare Access produces most often, a login redirect,
/// which is otherwise indistinguishable from a broken backend.
export function checkBackend(
  url: string | null,
  clientId: string | null,
  clientSecret: string | null,
): Promise<string> {
  return invoke<string>("check_backend", { url, clientId, clientSecret });
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

/// The unsent draft of a writing session, as the Rust side last wrote it.
///
/// The shape is entirely this side's business — `draft.rs` stores whatever
/// JSON it is handed and gives it back byte for byte. What it guarantees
/// is durability: an interrupted write leaves the previous draft intact,
/// which is the only property that matters when the thing being protected
/// is four hours of a meeting.
export async function loadDraft<T>(): Promise<T | null> {
  if (!runningInDesktopApp()) return readWebDraft<T>();
  return ((await invoke("draft_load")) as T | null) ?? null;
}

export async function saveDraft(draft: unknown): Promise<void> {
  if (!runningInDesktopApp()) return writeWebDraft(draft);
  await invoke("draft_save", { draft });
}

export async function clearDraft(): Promise<void> {
  if (!runningInDesktopApp()) {
    writeWebDraft(null);
    return;
  }
  await invoke("draft_clear");
}

/// `npm run dev` in a plain browser has no Rust side to ask. Session
/// storage is not durable in any sense that matters, but losing a draft
/// while developing is an inconvenience and being unable to open the
/// screen at all is not.
const WEB_DRAFT_KEY = "hippocampus.draft";

function readWebDraft<T>(): T | null {
  try {
    const raw = window.localStorage.getItem(WEB_DRAFT_KEY);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

function writeWebDraft(draft: unknown): void {
  try {
    if (draft === null) window.localStorage.removeItem(WEB_DRAFT_KEY);
    else window.localStorage.setItem(WEB_DRAFT_KEY, JSON.stringify(draft));
  } catch {
    // Private mode, or storage full. The draft simply will not persist.
  }
}
