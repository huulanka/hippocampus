//! Touch ID in front of the notes.
//!
//! The app holds everything its owner has ever said, on a laptop that
//! spends its day unlocked on a desk. A screen anyone can read by walking
//! past is the wrong default for that, so reading is behind the machine's
//! own authentication and capturing is not.
//!
//! **Capturing stays open on purpose.** Speaking a note *adds* something;
//! it reveals nothing. The global shortcut is the main path through this
//! app, and a gate in front of "press and speak" would cost the one
//! property the whole design is built around. Reading — the timeline,
//! search, entities, the graph, a capture's text and its recording — is
//! what is worth protecting, and that is what is gated.
//!
//! **The gate is enforced here, not in the webview.** A lock screen that
//! is only drawn is a picture of a lock: the requests behind it still
//! work. [`Guard::allows`] is what actually decides, and it is an
//! allow-list — a new endpoint is closed until someone says otherwise,
//! which is the right way round for this.
//!
//! The policy is `DeviceOwnerAuthentication` rather than
//! `…WithBiometrics`: after a failed fingerprint macOS offers the login
//! password. Biometrics-only would be stricter and would also lock the
//! owner out of their own notes on any Mac without Touch ID hardware —
//! an external keyboard is enough.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

/// What the user is told to authorise, shown by macOS in its own dialog.
/// Phrased as the thing being asked for, because the system prompt
/// already supplies "Hippocampus is trying to".
const REASON: &str = "unlock your notes";

/// How long the app may sit unattended before it locks itself again.
pub const DEFAULT_IDLE_SECONDS: u64 = 300;
/// Bounds on what the setting may be turned into. Not arbitrary: below a
/// minute the app would lock while you fetch a coffee, and above an hour
/// the guard stops being one for the case it exists for — a laptop left
/// open on a desk.
pub const MIN_IDLE_SECONDS: u64 = 60;
pub const MAX_IDLE_SECONDS: u64 = 3600;

/// Which authentications this machine can actually perform. Reported to
/// the webview so the settings screen can say "Touch ID" when there is a
/// sensor and "your password" when there is not, instead of promising
/// hardware that is not there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mechanism {
    TouchId,
    /// The device can authenticate, but not with a biometric sensor —
    /// the login password is the whole of it.
    Password,
    /// Nothing can be evaluated. The gate makes itself inert rather than
    /// locking someone out of their own notes.
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct LockStatus {
    pub mechanism: Mechanism,
    /// Whether the gate is switched on in settings.
    pub enabled: bool,
    /// Whether it is closed *right now*.
    pub locked: bool,
    pub idle_seconds: u64,
}

struct Inner {
    unlocked: bool,
    /// When the window last stopped being the front one. `None` means it
    /// is in front now, and the idle clock is not running.
    away_since: Option<Instant>,
}

pub struct LockState {
    inner: Mutex<Inner>,
}

impl Default for LockState {
    fn default() -> Self {
        Self::new()
    }
}

impl LockState {
    /// Starts locked. Anything else would mean the first launch after a
    /// restart is the one that shows everything.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                unlocked: false,
                away_since: None,
            }),
        }
    }

    pub fn is_unlocked(&self) -> bool {
        self.inner.lock().map(|i| i.unlocked).unwrap_or(false)
    }

    pub fn unlock(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.unlocked = true;
            inner.away_since = None;
        }
    }

    pub fn lock(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.unlocked = false;
            inner.away_since = None;
        }
    }

    /// Records that the window came to the front or left it. The idle
    /// clock runs only while it is away: a note you are reading should
    /// not vanish mid-sentence because you stopped typing.
    pub fn set_focused(&self, focused: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.away_since = if focused { None } else { Some(Instant::now()) };
        }
    }

    /// Locks if the window has been away longer than `idle`. Returns
    /// whether this call is what closed it, so the caller can tell the
    /// webview exactly once.
    pub fn lock_if_idle(&self, idle: Duration) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        if !inner.unlocked {
            return false;
        }
        let Some(away) = inner.away_since else {
            return false;
        };
        if away.elapsed() < idle {
            return false;
        }
        inner.unlocked = false;
        inner.away_since = None;
        true
    }
}

/// Whether the guard is actually standing, given the setting and what the
/// machine can do.
///
/// Two rules, and both matter. Silence means on: a settings file written
/// before this existed belongs to someone who has never been asked, and
/// the safe reading of silence about a guard is that they want it. And a
/// machine that cannot authenticate at all disarms it entirely — without
/// that, a Mac whose Touch ID has stopped working would hold its owner
/// out of their own notes with no way back in.
pub fn armed(enabled: Option<bool>, mechanism: Mechanism) -> bool {
    enabled.unwrap_or(true) && mechanism != Mechanism::None
}

/// Whether a request may go out while the notes are locked.
///
/// An allow-list, and a short one. Two things are open: recording a new
/// capture, which only ever adds, and asking the backend its version,
/// which the settings screen shows and which says nothing about anyone's
/// notes. Everything else — including every read of a capture that was
/// just written — waits for the gate.
pub fn allows(method: &str, path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    match method.to_ascii_uppercase().as_str() {
        "POST" => path == "/captures",
        "GET" => path == "/version" || path == "/health",
        _ => false,
    }
}

/// What the webview is told when a request is refused. Deliberately not
/// an HTTP-shaped error: nothing reached the network, and pretending
/// otherwise would send the caller looking at the backend.
pub const REFUSED: &str = "the notes are locked";

/// The whole decision, in one place: may this request go out?
///
/// Separated from the request path so that what actually guards the notes
/// can be tested without a running app, a Keychain or a Touch ID sensor.
/// What is left at the call site is a single `if`, which is about as much
/// as should ever be untested in something whose job is to say no.
pub fn refusal(armed: bool, unlocked: bool, method: &str, path: &str) -> Option<&'static str> {
    if !armed || unlocked || allows(method, path) {
        return None;
    }
    Some(REFUSED)
}

/// A recording has no allow-listed case at all: it is the most revealing
/// thing this app holds, so it waits for the gate, always.
pub fn refusal_for_audio(armed: bool, unlocked: bool) -> Option<&'static str> {
    if !armed || unlocked {
        return None;
    }
    Some(REFUSED)
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2_foundation::NSString;
    use objc2_local_authentication::{LABiometryType, LAContext, LAPolicy};

    use super::{Mechanism, REASON};

    /// `DeviceOwnerAuthentication`, not `…WithBiometrics` — see the
    /// module docs for why the password fallback is not optional.
    const POLICY: LAPolicy = LAPolicy::DeviceOwnerAuthentication;

    pub fn mechanism() -> Mechanism {
        // SAFETY: LAContext is an ordinary Objective-C object with no
        // initialisation requirements beyond `new`, and every call below
        // is on the thread that created it.
        unsafe {
            let context = LAContext::new();
            if context.canEvaluatePolicy_error(POLICY).is_err() {
                return Mechanism::None;
            }
            // `biometryType` is only meaningful once a policy has been
            // evaluated or checked, which the line above just did.
            match context.biometryType() {
                LABiometryType::TouchID => Mechanism::TouchId,
                _ => Mechanism::Password,
            }
        }
    }

    /// Blocks until the user answers the system dialog.
    ///
    /// LocalAuthentication is callback-based, so the completion block
    /// hands its answer back over a channel and this waits on it. The
    /// caller runs it off the main thread; the dialog itself is put up by
    /// the system, not by this app's event loop, so nothing here has to
    /// be on the main thread for it to appear.
    pub fn authenticate() -> Result<bool, String> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<bool, String>>();

        // SAFETY: the block is called exactly once by LocalAuthentication
        // and only sends on the channel, and `rx` outlives it because
        // this function blocks on it below.
        unsafe {
            let context = LAContext::new();
            let reason = NSString::from_str(REASON);
            let reply = block2::RcBlock::new(
                move |granted: objc2::runtime::Bool, error: *mut objc2_foundation::NSError| {
                    let outcome = if granted.as_bool() {
                        Ok(true)
                    } else if error.is_null() {
                        Ok(false)
                    } else {
                        let message = (*error).localizedDescription().to_string();
                        Err(message)
                    };
                    // A closed receiver means the app is going away; there is
                    // nobody left to tell.
                    let _ = tx.send(outcome);
                },
            );
            context.evaluatePolicy_localizedReason_reply(POLICY, &reason, &reply);
        }

        rx.recv()
            .unwrap_or_else(|_| Err("the authentication dialog went away".to_string()))
    }
}

/// Everywhere that is not macOS the gate reports itself unavailable, and
/// [`LockState`] is then never armed. This crate only ships on macOS
/// today; the point of the stub is that `cargo check` on another platform
/// tells the truth rather than failing to build.
#[cfg(not(target_os = "macos"))]
mod platform {
    use super::Mechanism;

    pub fn mechanism() -> Mechanism {
        Mechanism::None
    }

    pub fn authenticate() -> Result<bool, String> {
        Err("this platform has no device authentication".to_string())
    }
}

pub use platform::{authenticate, mechanism};

/// The event the webview listens for when the app locks itself. Pushed
/// rather than polled: the whole point is that the screen goes away while
/// nobody is asking it anything.
pub const LOCKED_EVENT: &str = "hippocampus://locked";

#[tauri::command]
pub fn lock_status(
    settings: tauri::State<'_, crate::settings::SettingsState>,
    lock: tauri::State<'_, LockState>,
) -> LockStatus {
    settings.lock_status(&lock)
}

/// Puts the system dialog up and opens the gate if it is answered.
///
/// Run off the calling thread because it blocks until the user decides,
/// which may be never — a blocked IPC worker would take the rest of the
/// app's commands with it.
#[tauri::command]
pub async fn unlock(
    settings: tauri::State<'_, crate::settings::SettingsState>,
    lock: tauri::State<'_, LockState>,
) -> Result<LockStatus, String> {
    if !settings.lock_armed() {
        lock.unlock();
        return Ok(settings.lock_status(&lock));
    }

    let granted = tauri::async_runtime::spawn_blocking(authenticate)
        .await
        .map_err(|err| format!("the authentication never finished: {err}"))??;

    if granted {
        lock.unlock();
    }
    Ok(settings.lock_status(&lock))
}

/// Closes the gate by hand, for leaving the desk without waiting out the
/// idle timer.
#[tauri::command]
pub fn lock_now(
    settings: tauri::State<'_, crate::settings::SettingsState>,
    lock: tauri::State<'_, LockState>,
) -> LockStatus {
    lock.lock();
    settings.lock_status(&lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capturing_is_open_and_reading_is_not() {
        assert!(allows("POST", "/captures"));
        assert!(allows("GET", "/version"));

        assert!(!allows("GET", "/captures"));
        assert!(!allows("GET", "/captures/abc"));
        assert!(!allows("GET", "/captures/abc/echo"));
        assert!(!allows("GET", "/entities"));
        assert!(!allows("GET", "/graph"));
        assert!(!allows("GET", "/search?q=anything"));
        assert!(!allows("GET", "/resurface"));
    }

    /// A query string must not be able to turn a closed path into an open
    /// one, and an open path must stay open when it carries one.
    #[test]
    fn the_query_string_is_not_part_of_the_decision() {
        assert!(!allows("GET", "/captures?limit=1"));
        assert!(allows("GET", "/version?cachebust=1"));
    }

    /// Everything that changes something a capture already said is a
    /// reading action first — you cannot correct what you cannot see.
    #[test]
    fn editing_and_deleting_wait_for_the_gate() {
        assert!(!allows("DELETE", "/captures/abc"));
        assert!(!allows("PUT", "/captures/abc/transcript"));
        assert!(!allows("POST", "/captures/abc/transcript"));
    }

    #[test]
    fn a_machine_that_cannot_authenticate_is_never_guarded() {
        // The escape hatch. Whatever the setting says, there has to be a
        // way back in.
        assert!(!armed(Some(true), Mechanism::None));
        assert!(!armed(None, Mechanism::None));
    }

    #[test]
    fn never_having_been_asked_counts_as_yes() {
        assert!(armed(None, Mechanism::TouchId));
        assert!(armed(None, Mechanism::Password));
    }

    #[test]
    fn switching_it_off_switches_it_off() {
        assert!(!armed(Some(false), Mechanism::TouchId));
    }

    /// The four states the request path can be in, and the one of them
    /// that says no.
    #[test]
    fn only_a_closed_guard_refuses_a_read() {
        // Armed and closed: the case the whole feature exists for.
        assert_eq!(refusal(true, false, "GET", "/entities"), Some(REFUSED));
        // Armed and open: unlocked is unlocked.
        assert_eq!(refusal(true, true, "GET", "/entities"), None);
        // Switched off, or a machine that cannot authenticate.
        assert_eq!(refusal(false, false, "GET", "/entities"), None);
        assert_eq!(refusal(false, true, "GET", "/entities"), None);
    }

    #[test]
    fn a_closed_guard_still_lets_a_capture_through() {
        assert_eq!(refusal(true, false, "POST", "/captures"), None);
    }

    #[test]
    fn a_recording_waits_for_the_gate_with_no_exception() {
        assert_eq!(refusal_for_audio(true, false), Some(REFUSED));
        assert_eq!(refusal_for_audio(true, true), None);
        assert_eq!(refusal_for_audio(false, false), None);
    }

    #[test]
    fn it_starts_locked() {
        assert!(!LockState::new().is_unlocked());
    }

    #[test]
    fn the_idle_clock_runs_only_while_the_window_is_away() {
        let state = LockState::new();
        state.unlock();

        // In front: no clock, so nothing locks however long it sits.
        state.set_focused(true);
        assert!(!state.lock_if_idle(Duration::ZERO));
        assert!(state.is_unlocked());

        state.set_focused(false);
        assert!(!state.lock_if_idle(Duration::from_secs(300)));
        assert!(state.lock_if_idle(Duration::ZERO));
        assert!(!state.is_unlocked());
    }

    /// The webview is told once, not on every tick of the timer.
    #[test]
    fn locking_reports_only_the_transition() {
        let state = LockState::new();
        state.unlock();
        state.set_focused(false);
        assert!(state.lock_if_idle(Duration::ZERO));
        assert!(!state.lock_if_idle(Duration::ZERO));
    }

    /// Coming back to the window restarts the clock rather than leaving a
    /// stale one that locks the moment you look away again.
    #[test]
    fn returning_to_the_window_clears_the_clock() {
        let state = LockState::new();
        state.unlock();
        state.set_focused(false);
        state.set_focused(true);
        assert!(!state.lock_if_idle(Duration::ZERO));
    }
}
