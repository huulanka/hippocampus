//! Tauri shell for the Hippocampus client.
//!
//! Its whole job is to make capturing cost nothing: a global shortcut
//! brings the capture field up from wherever the user is, and Escape sends
//! it away again. Everything else lives in the webview.

// The menu bar, the calendar and Touch ID are Mac-only callers of a good
// part of this crate; on a phone that code is present but unreached.
#![cfg_attr(mobile, allow(dead_code))]

// On a phone, `asr`, `recorder` and `tray` are stand-ins with the same
// surface: speech and the menu bar exist only on the Mac for now
// (docs/iphone.md).
#[cfg_attr(mobile, path = "mobile/asr.rs")]
mod asr;
mod backend;
mod calendar;
mod capture;
mod draft;
mod foresight;
mod keychain;
mod lock;
pub mod microphone;
mod outbox;
#[cfg_attr(mobile, path = "mobile/recorder.rs")]
mod recorder;
mod review;
mod settings;
mod sync;
#[cfg_attr(mobile, path = "mobile/tray.rs")]
mod tray;

use tauri::{Emitter, Manager};
#[cfg(desktop)]
use tauri_plugin_global_shortcut::GlobalShortcutExt;
#[cfg(desktop)]
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_log::{Target, TargetKind};

use settings::SettingsState;

/// What macOS currently thinks about us and the microphone. Exposed so
/// `cargo run --example microphone_status` can answer the one question
/// that no retry inside the app can: has permission been refused?
pub fn microphone_permission() -> microphone::Permission {
    microphone::current()
}

/// Event the webview listens for to jump to a fresh capture field. The
/// frontend treats *every* firing of this as a press of the shortcut —
/// which starts recording if nothing is already in flight, and stops it
/// if a recording is — so this must only ever be emitted for an actual
/// press of the capture shortcut, never for "the app should be visible."
#[cfg(desktop)]
const FOCUS_EVENT: &str = "hippocampus://focus-capture";

/// Brings the main window forward without touching recording. What the
/// tray icon's left click, its "Open Hippocampus" menu item, and a second
/// launch caught by the single-instance guard all want: come back into
/// view, and nothing more. Failures are logged rather than propagated —
/// a click that does nothing is a bad moment, but one that panics takes
/// the whole app with it.
#[cfg(desktop)]
pub(crate) fn reveal_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("asked to show the window, but it is gone");
        return;
    };

    if let Err(err) = window.show() {
        log::warn!("could not show window: {err}");
    }
    if let Err(err) = window.set_focus() {
        log::warn!("could not focus window: {err}");
    }
}

/// The capture shortcut's actual behaviour: bring the window forward
/// *and* tell the webview a press just happened, which is what starts
/// (or stops) a recording. Only the global shortcut handler below may
/// call this — anything else that wants the window back wants
/// [`reveal_window`], not this.
#[cfg(desktop)]
fn summon_capture(app: &tauri::AppHandle) {
    reveal_window(app);
    if let Err(err) = app.emit(FOCUS_EVENT, ()) {
        log::warn!("could not notify webview: {err}");
    }
}

/// How often the idle clock is checked. Coarse on purpose: the thing
/// being measured is minutes of absence, and a timer that wakes the
/// machine four times a second to find out nothing has changed is a
/// battery cost with no reader.
const IDLE_TICK: std::time::Duration = std::time::Duration::from_secs(15);

/// Locks the app again once it has been left alone long enough.
///
/// A timer rather than a check when the window is next touched: the case
/// this exists for is a laptop left open on a desk with the timeline
/// still on the screen. Waiting for someone to click would mean the notes
/// are readable for exactly as long as nobody interacts with them, which
/// is the opposite of the guarantee.
fn watch_for_idleness(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(IDLE_TICK).await;

            let idle = app.state::<SettingsState>().lock_idle();
            if !app.state::<SettingsState>().lock_armed() {
                continue;
            }
            if app.state::<lock::LockState>().lock_if_idle(idle) {
                log::info!("locked again after {}s unattended", idle.as_secs());
                // Told, not polled: the screen has to go away while
                // nobody is asking it anything.
                if let Err(err) = app.emit(lock::LOCKED_EVENT, ()) {
                    log::warn!("could not tell the webview it locked: {err}");
                }
            }
        }
    });
}

/// What only a Mac has: a second launch to catch, a login item and a
/// system-wide shortcut. On a phone the app is started by the system and
/// by the Action Button (docs/iphone.md, I5), and none of the three
/// exists.
#[cfg(desktop)]
fn desktop_plugins(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        // Registered before anything else, per the plugin's own
        // requirement: a second launch (double-clicking the app again,
        // or opening it from Spotlight while it is already running in
        // the menu bar) is caught here and turned into "bring the
        // existing window forward" instead of a second process fighting
        // the first one for the same global shortcut, outbox directory
        // and backend connection.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            reveal_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    // Fire on press only; the release event would otherwise
                    // summon the window a second time on every use.
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    // Compared against the current setting rather than
                    // assumed: the user can change it while the app runs.
                    if shortcut == &app.state::<SettingsState>().capture_shortcut() {
                        summon_capture(app);
                    }
                })
                .build(),
        )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = desktop_plugins(builder);
    // Recording and Parakeet on the phone, in Swift (`plugins/speech`); on
    // a Mac it registers and does nothing.
    let builder = builder.plugin(tauri_plugin_hippocampus_speech::init());

    builder
        // Registered first so nothing logged during setup is lost. Writes
        // to stdout (visible under `tauri dev`) and to a rolling file
        // under the OS log dir, which is what the Settings screen's "Open
        // Logs" button points at — the same fix as the backend's, for the
        // same reason: this stops being a terminal session once it leaves
        // this machine.
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // The idle clock runs only while the window is not the front
        // one. A note you are reading should not vanish mid-sentence
        // because you stopped typing for five minutes; a laptop you
        // walked away from is a different thing entirely, and this is how
        // the two are told apart.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                window
                    .app_handle()
                    .state::<lock::LockState>()
                    .set_focused(*focused);
            }
            // The red close button hides the window rather than ending
            // the process — the whole point of living in the menu bar is
            // that the app is still there, one click away, after the
            // window that happened to be open is gone. Quitting is now a
            // deliberate act, only offered from the tray menu.
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(err) = window.hide() {
                    log::warn!("could not hide window on close: {err}");
                }
            }
        })
        .manage(capture::CaptureState::new())
        .manage(backend::BackendClient::new())
        // Starts locked. Anything else would mean the first launch after
        // a restart is the one that shows everything.
        .manage(lock::LockState::new())
        .invoke_handler(tauri::generate_handler![
            capture::speech_available,
            capture::start_recording,
            capture::stop_recording,
            capture::cancel_recording,
            capture::capture_text,
            capture::speech_model,
            capture::download_speech_model,
            capture::take_record_request,
            sync::outbox_status,
            sync::sync_now,
            backend::api_request,
            backend::api_audio,
            backend::check_backend,
            settings::get_settings,
            #[cfg(desktop)]
            settings::set_capture_shortcut,
            settings::set_backend_url,
            settings::set_cf_access_credentials,
            settings::set_lock_enabled,
            settings::set_lock_idle_seconds,
            settings::set_review_schedule,
            settings::set_foresight,
            foresight::foresight_status,
            foresight::foresight_answered,
            foresight::foresight_refresh,
            foresight::calendar_access,
            foresight::request_calendar_access,
            foresight::list_calendars,
            foresight::open_calendar_privacy,
            #[cfg(desktop)]
            settings::autostart_enabled,
            #[cfg(desktop)]
            settings::set_autostart_enabled,
            lock::lock_status,
            lock::unlock,
            lock::lock_now,
            draft::draft_load,
            draft::draft_save,
            draft::draft_clear,
        ])
        .setup(|app| {
            #[cfg(mobile)]
            asr::install(app.handle());

            // Settings are loaded before the shortcut is registered, and
            // the handler reads them back out of managed state, so a
            // shortcut changed at runtime takes effect without a restart.
            let path = settings::settings_path(app.handle())?;
            app.manage(SettingsState::load(path));

            // A missing shortcut registration must not stop the app from
            // starting — the window still works, just without the hotkey.
            #[cfg(desktop)]
            if let Err(err) = app
                .global_shortcut()
                .register(app.state::<SettingsState>().capture_shortcut())
            {
                log::warn!("could not register the capture shortcut: {err}");
            }

            // The outbox before anything else in this block that could
            // fail: whatever else is wrong with this launch, captures
            // spoken during it have to have somewhere to land.
            let outbox = std::sync::Arc::new(outbox::Outbox::new(
                app.path().app_data_dir()?.join("outbox"),
            ));
            let waiting = outbox.counts().waiting;
            if waiting > 0 {
                log::info!("{waiting} captures were left waiting to sync; retrying them");
            }
            app.manage(sync::SyncState::new(outbox));
            sync::watch(app.handle().clone());

            watch_for_idleness(app.handle().clone());
            review::watch(app.handle().clone());

            // Before the tray, which asks it whether to sparkle.
            app.manage(foresight::ForesightState::load(foresight::state_path(
                app.handle(),
            )?));
            foresight::watch(app.handle().clone());

            // No Dock icon, no Cmd+Tab entry: the menu bar is now the
            // one place this app lives when its window is not open,
            // and a Dock icon sitting next to it would just be a second,
            // redundant way to ask for the same window.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(desktop)]
            tray::build(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
