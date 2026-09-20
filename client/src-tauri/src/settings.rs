//! Settings that outlive a single run of the app.
//!
//! Only one thing lives here so far: which key summons the capture field.
//! It is stored as a Tauri accelerator string ("Super+Shift+KeyH") rather
//! than as a parsed shortcut, because that is the form both the plugin and
//! a human editing the file by hand can read.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// Cmd+Shift+H, chosen because macOS leaves it alone: Cmd+Space and
/// Cmd+Shift+Space are Spotlight and input-source switching, and Option
/// combinations collide with text input on a German keyboard layout.
pub const DEFAULT_CAPTURE_SHORTCUT: &str = "Super+Shift+KeyH";

const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub capture_shortcut: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_shortcut: DEFAULT_CAPTURE_SHORTCUT.to_string(),
        }
    }
}

pub struct SettingsState {
    current: Mutex<Settings>,
    path: PathBuf,
}

impl SettingsState {
    /// Reads the settings file, falling back to defaults.
    ///
    /// A corrupt or unreadable file is reported and then ignored rather
    /// than being allowed to stop the app: losing a preference is a small
    /// annoyance, not being able to capture at all is not.
    pub fn load(path: PathBuf) -> Self {
        let current = match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|err| {
                eprintln!("settings file at {} is unreadable: {err}", path.display());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };

        Self {
            current: Mutex::new(current),
            path,
        }
    }

    pub fn snapshot(&self) -> Settings {
        self.current
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    /// The shortcut as the plugin wants it, or the default if what is
    /// stored no longer parses.
    pub fn capture_shortcut(&self) -> Shortcut {
        parse(&self.snapshot().capture_shortcut).unwrap_or_else(|_| {
            parse(DEFAULT_CAPTURE_SHORTCUT).expect("the default shortcut must parse")
        })
    }

    fn persist(&self, settings: &Settings) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_vec_pretty(settings)?)?;
        Ok(())
    }
}

fn parse(accelerator: &str) -> anyhow::Result<Shortcut> {
    accelerator
        .parse::<Shortcut>()
        .map_err(|err| anyhow::anyhow!("{err}"))
}

/// The file the settings live in, next to the app's other config.
pub fn settings_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(FILE_NAME))
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, SettingsState>) -> Settings {
    state.snapshot()
}

/// Registers a new capture shortcut and remembers it.
///
/// The new shortcut is registered before the old one is forgotten, so a
/// combination the system has already claimed leaves the user with the
/// shortcut they had rather than with none at all.
#[tauri::command]
pub fn set_capture_shortcut(
    app: tauri::AppHandle,
    state: tauri::State<'_, SettingsState>,
    accelerator: String,
) -> Result<Settings, String> {
    let wanted = parse(&accelerator).map_err(|_| format!("{accelerator} is not a shortcut"))?;
    let previous = state.capture_shortcut();

    if wanted == previous {
        return Ok(state.snapshot());
    }

    let shortcuts = app.global_shortcut();
    shortcuts.register(wanted).map_err(|err| {
        format!("that combination could not be registered — something else may own it ({err})")
    })?;
    if let Err(err) = shortcuts.unregister(previous) {
        eprintln!("could not release the previous shortcut: {err}");
    }

    let updated = Settings {
        capture_shortcut: wanted.into_string(),
    };

    if let Ok(mut current) = state.current.lock() {
        *current = updated.clone();
    }
    if let Err(err) = state.persist(&updated) {
        // The shortcut works right now; it just will not survive a restart.
        // Worth saying, not worth undoing a change the user asked for.
        eprintln!("could not write settings: {err}");
    }

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shortcut_parses() {
        assert!(parse(DEFAULT_CAPTURE_SHORTCUT).is_ok());
    }

    #[test]
    fn accelerators_round_trip_through_the_stored_form() {
        let parsed = parse("Super+Shift+KeyH").unwrap();
        let stored = parsed.into_string();
        assert_eq!(parse(&stored).unwrap(), parsed);
    }

    #[test]
    fn nonsense_is_rejected_rather_than_silently_ignored() {
        assert!(parse("Ctrl+Shift+").is_err());
        assert!(parse("NotAKey").is_err());
    }

    #[test]
    fn unreadable_settings_fall_back_to_the_default() {
        let dir = std::env::temp_dir().join("hippocampus-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.json");
        std::fs::write(&path, b"{ not json").unwrap();

        let state = SettingsState::load(path.clone());
        assert_eq!(state.snapshot().capture_shortcut, DEFAULT_CAPTURE_SHORTCUT);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let state = SettingsState::load(PathBuf::from("/nonexistent/hippocampus/settings.json"));
        assert_eq!(state.snapshot().capture_shortcut, DEFAULT_CAPTURE_SHORTCUT);
    }
}
