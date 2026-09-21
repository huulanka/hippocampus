//! Tauri shell for the Hippocampus client.
//!
//! Its whole job is to make capturing cost nothing: a global shortcut
//! brings the capture field up from wherever the user is, and Escape sends
//! it away again. Everything else lives in the webview.

mod asr;
mod capture;
pub mod microphone;
mod recorder;
mod settings;

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_log::{Target, TargetKind};

use settings::SettingsState;

/// What macOS currently thinks about us and the microphone. Exposed so
/// `cargo run --example microphone_status` can answer the one question
/// that no retry inside the app can: has permission been refused?
pub fn microphone_permission() -> microphone::Permission {
    microphone::current()
}

/// Event the webview listens for to jump to a fresh capture field.
const FOCUS_EVENT: &str = "hippocampus://focus-capture";

/// Brings the main window forward and asks the webview for a blank capture
/// field. Failures are logged rather than propagated: a shortcut that does
/// nothing is a bad evening, but a shortcut that panics takes the whole
/// app with it.
fn summon_capture(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("global shortcut fired but the main window is gone");
        return;
    };

    if let Err(err) = window.show() {
        log::warn!("could not show window: {err}");
    }
    if let Err(err) = window.set_focus() {
        log::warn!("could not focus window: {err}");
    }
    if let Err(err) = app.emit(FOCUS_EVENT, ()) {
        log::warn!("could not notify webview: {err}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        .manage(capture::CaptureState::new())
        .invoke_handler(tauri::generate_handler![
            capture::speech_available,
            capture::start_recording,
            capture::stop_recording,
            capture::cancel_recording,
            settings::get_settings,
            settings::set_capture_shortcut,
            settings::set_backend_url,
            settings::set_cf_access_credentials,
        ])
        .setup(|app| {
            // Settings are loaded before the shortcut is registered, and
            // the handler reads them back out of managed state, so a
            // shortcut changed at runtime takes effect without a restart.
            let path = settings::settings_path(app.handle())?;
            let state = SettingsState::load(path);
            let shortcut = state.capture_shortcut();
            app.manage(state);

            // A missing shortcut registration must not stop the app from
            // starting — the window still works, just without the hotkey.
            if let Err(err) = app.global_shortcut().register(shortcut) {
                log::warn!("could not register the capture shortcut: {err}");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
