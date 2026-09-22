# ADR 0011: Reading is guarded, capturing is not

## Status
Accepted (2026-09-22)

## Context
The app holds every thought its owner has ever recorded, and it now
installs with `brew` onto a laptop that spends its working day open on a
desk. Anyone walking past reads the timeline. Nothing in the system asked
who was looking.

macOS already answers "is this the owner" better than this app could:
LocalAuthentication puts up the system's own dialog, checks the Touch ID
sensor, and falls back to the login password. The question was never
*whether* to use it. It was what to put behind it.

The obvious answer — a lock screen in front of the whole app — collides
with the single decision the rest of this design is built on: **voice
capture is the main path, not a second option.** The global shortcut
exists so a thought can be spoken the moment it arrives. A prompt in
front of that turns a two-second act into a five-second one, and a
capture system you hesitate before using is a capture system that stops
being used.

## Decision
**Reading is guarded. Capturing is not.**

Speaking or typing a note only ever *adds*; it reveals nothing that was
not already in the person's head. Reading it back — the timeline, search,
the entity pages, the graph, a capture's text, and above all its
recording — is the part worth a gate, and it is the only part that has
one. Settings is open too: it holds no notes, and being shut out of the
screen that configures the backend by a guard you cannot reach to switch
off is a trap. Switching the guard off from there authenticates first.

**The gate is enforced in Rust, not drawn in the webview.** `lock::allows`
decides, and it is an *allow-list*: `POST /captures` and `GET /version`,
nothing else. A new endpoint is closed until somebody says otherwise,
which is the right way round for this. A lock screen that is only
rendered is a picture of a lock — the requests behind it still work, and
the difference only shows up on the day it matters.

**The capture response is stripped while locked.** This is the one place
where writing hands back reading: the backend answers a new capture with
the earlier captures closest to it, verbatim. Left alone, the one
allow-listed write would have walked the echo straight past the gate. The
capture is still stored and still judged; the answer is quiet about it,
and the capture screen says so rather than showing an empty space where
an echo would be.

**The policy is `DeviceOwnerAuthentication`, not
`…WithBiometrics`.** After a failed fingerprint, macOS offers the login
password. Biometrics-only would be stricter and would also lock the owner
out of their own notes on any Mac without Touch ID hardware — an external
keyboard is enough. The same instinct runs through the whole feature: a
machine that cannot authenticate at all disarms the guard rather than
sealing it.

**It re-locks on a clock that only runs while the window is not in
front.** A note you are reading must not vanish mid-sentence because you
stopped typing; a laptop you walked away from is a different thing, and
that is the case this exists for. Five minutes by default, changeable,
and a timer rather than a check on next interaction — waiting for someone
to click would mean the notes stay readable exactly as long as nobody
touches them, which is the opposite of the guarantee.

## Consequences
- The threat this addresses is an unattended, *unlocked* Mac. It is not
  disk encryption and does not pretend to be: the database is on a NAS,
  the audio is on disk, and anyone with the login password has both. What
  it removes is the shoulder-surfing case, which is the one that actually
  happens.
- `tauri-plugin-biometric` is not usable — it exists only for Android and
  iOS. The binding is `objc2-local-authentication`, the same objc2
  generation as the AVFoundation bindings the microphone already uses, so
  this crate keeps one Objective-C bridge rather than two.
- Touch ID cannot be exercised from a development build in the way the
  rest of the UI can: like the microphone (ADR 0004's consequence), it
  needs the real app. It was confirmed by hand in a running build.
- If the guard is ever switched on for a machine that later loses its
  authentication, `settings.json` is the escape hatch:
  `"lock_enabled": false`.
