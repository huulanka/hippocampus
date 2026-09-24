# ADR 0012: The app lives in the menu bar; only the shortcut starts a recording

## Status
Accepted (2026-09-22)

## Context
Until now Hippocampus was a normal window: quitting it killed the global
shortcut along with it, and a Mac restart left the shortcut dead until
someone remembered to relaunch the app by hand. The obvious fix
is a menu bar icon, a window that hides rather than dies when
closed, autostart, and no second process fighting the first one for the
same shortcut, outbox directory and backend connection.

Building it surfaced a real bug rather than a design question. The
global shortcut's handler does two things at once: it shows the window
*and* it emits `hippocampus://focus-capture`, which the frontend reads as
"the shortcut was pressed" and answers by starting a recording (pressing
it again stops one — see the comment at `CaptureScreen.tsx`'s `summons`
effect). That double duty is deliberate for the shortcut: press once,
speak, press again, done. Reusing the same function for the tray icon's
left click, its "Open Hippocampus" menu item, and the single-instance
guard's "someone tried to launch a second copy" callback meant every one
of those silently started listening — confirmed live, by the orange
"microphone in use" pill appearing in the menu bar without anyone having
pressed the shortcut or touched a record button.

## Decision
**Two functions, not one.** `reveal_window` shows the window and focuses
it — nothing else. `summon_capture` does that *and* emits the shortcut
event, and only the global shortcut's own handler may call it. The tray's
left click, its menu item, and the single-instance relaunch callback all
call `reveal_window`. Bringing the app into view must never be mistaken
for pressing the record button, no matter how many doors lead to "the
window is visible now."

**No Dock icon.** `ActivationPolicy::Accessory`, set once in `setup()`.
The menu bar is the one place this app lives when its window is closed;
a Dock icon next to it would be a second, redundant way to ask for the
same window. The classic menu-bar-app shape is the point, not "also
keep the Dock icon."

**Closing the window hides it; only the tray's "Quit" ends the process.**
`WindowEvent::CloseRequested` calls `api.prevent_close()` and hides the
window instead. Quitting is now a deliberate act with its own menu item,
not a side effect of the red button.

**Autostart defaults off.** Unlike the lock in ADR 0011, silence here is
not read as consent — registering a login item is a system-visible
change nobody asked for yet, and macOS itself will tell the user about it
anyway (Settings → General → Login Items). It is one settings toggle,
read straight from the OS's own launch-agent registration
(`tauri-plugin-autostart`'s `is_enabled()`) rather than mirrored in
`settings.json`, so there is no second copy of that state to fall out of
sync with the truth.

**The brain is a literal bitmap in source, not a PNG.** A menu bar glyph
is a handful of pixels either way, and the first attempt — a 20×20 grid
with single-pixel gyri notches — read fine in an ASCII preview and turned
to visual noise at the size a menu bar actually renders it, confirmed by
the person looking at the real icon on a real screen. The bitmap in
`tray.rs` now keeps every stroke at least two logical pixels thick for
exactly that reason: it has to survive the downscale and macOS's own
antialiasing, not just this file's zoom level. It pulses — two rows of
the groove between its lobes lighting up, one step at a time — only while
a recording is actually in progress, and holds still otherwise; a menu
bar item that moves for no reason is one that gets tuned out.

## Consequences
- A tray icon, its menu, autostart, and single-instance guard cannot be
  exercised meaningfully under `cargo run`/`tauri dev` — like the
  microphone (ADR 0004) and Touch ID (ADR 0011), this needed a real
  `.app` bundle (`npx tauri build --debug --bundles app`) to confirm on
  screen, including the bug above, which the unbundled dev process never
  surfaced because nothing was watching the menu bar for it at the time.
- `crate::summon_capture` is now private to `lib.rs`; anything elsewhere
  that wants the window back imports `reveal_window`, not it. A future
  caller reaching for "bring the app forward" will find the function
  named for exactly that, and has to go out of its way to also start a
  recording.
- The menu bar icon is presently the only place in the app with an
  intentionally drawn "8-bit brain." The window and Dock icon are still
  Tauri's default — extending the same bitmap approach to those is future
  work, not done here.
