//! Settings that outlive a single run of the app.
//!
//! Which key summons the capture field, stored as a Tauri accelerator
//! string ("Super+Shift+KeyH") rather than as a parsed shortcut, because
//! that is the form both the plugin and a human editing the file by hand
//! can read. Which backend the client talks to — `None` means the
//! built-in default, so a fresh install needs no configuration to work
//! against a locally-run backend. And, once that backend sits behind
//! Cloudflare Access rather than on localhost, the Service Token
//! credentials that get it past Access without a browser login — plain
//! JSON on disk, same trust boundary as everything else here: this Mac,
//! this user.

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
    /// `None` means "use the built-in default", not "unset" — keeps a
    /// fresh `settings.json` from an older version (which has no such
    /// field) loading as if the user had never touched this.
    #[serde(default)]
    pub backend_url: Option<String>,
    /// Cloudflare Access Service Token, sent as `CF-Access-Client-Id` /
    /// `CF-Access-Client-Secret`. Both or neither — a Service Token is
    /// only useful as a pair, and the API layer treats one set without
    /// the other as absent rather than sending a half-authenticated
    /// request.
    #[serde(default)]
    pub cf_access_client_id: Option<String>,
    #[serde(default)]
    pub cf_access_client_secret: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_shortcut: DEFAULT_CAPTURE_SHORTCUT.to_string(),
            backend_url: None,
            cf_access_client_id: None,
            cf_access_client_secret: None,
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
                log::warn!("settings file at {} is unreadable: {err}", path.display());
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
        log::warn!("could not release the previous shortcut: {err}");
    }

    let updated = Settings {
        capture_shortcut: wanted.into_string(),
        ..state.snapshot()
    };

    if let Ok(mut current) = state.current.lock() {
        *current = updated.clone();
    }
    if let Err(err) = state.persist(&updated) {
        // The shortcut works right now; it just will not survive a restart.
        // Worth saying, not worth undoing a change the user asked for.
        log::warn!("could not write settings: {err}");
    }

    Ok(updated)
}

/// Points the client at a different backend, or back at the built-in
/// default when given an empty string.
///
/// No reachability check happens here — the caller does that against
/// `/health` before committing to a value, so a typo does not lock the
/// user out of the settings screen that would let them fix it.
#[tauri::command]
pub fn set_backend_url(
    state: tauri::State<'_, SettingsState>,
    url: Option<String>,
) -> Result<Settings, String> {
    let trimmed = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());

    let updated = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        current.backend_url = trimmed;
        current.clone()
    };

    if let Err(err) = state.persist(&updated) {
        log::warn!("could not write settings: {err}");
    }

    Ok(updated)
}

/// Sets or clears the Cloudflare Access Service Token. Pass `None` (or an
/// empty string) for either field to clear both — a stored ID with no
/// secret, or vice versa, is not a state the client should ever send.
#[tauri::command]
pub fn set_cf_access_credentials(
    state: tauri::State<'_, SettingsState>,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<Settings, String> {
    let id = client_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let secret = client_secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let (id, secret) = match (id, secret) {
        (Some(id), Some(secret)) => (Some(id), Some(secret)),
        _ => (None, None),
    };

    let updated = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        current.cf_access_client_id = id;
        current.cf_access_client_secret = secret;
        current.clone()
    };

    if let Err(err) = state.persist(&updated) {
        log::warn!("could not write settings: {err}");
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

    /// A `settings.json` written by a version of the app that predates
    /// `backend_url` must still load — that is what `#[serde(default)]`
    /// on the field is for.
    #[test]
    fn a_settings_file_without_backend_url_loads_as_the_default() {
        let dir = std::env::temp_dir().join("hippocampus-settings-test-backend-url-absent");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, br#"{"capture_shortcut":"Super+Shift+KeyH"}"#).unwrap();

        let state = SettingsState::load(path.clone());
        assert_eq!(state.snapshot().backend_url, None);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_saved_backend_url_survives_a_reload() {
        let dir = std::env::temp_dir().join("hippocampus-settings-test-backend-url-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        let state = SettingsState::load(path.clone());
        let mut settings = state.snapshot();
        settings.backend_url = Some("https://hippocampus.example.com".to_string());
        state.persist(&settings).unwrap();

        let reloaded = SettingsState::load(path.clone());
        assert_eq!(
            reloaded.snapshot().backend_url,
            Some("https://hippocampus.example.com".to_string())
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_saved_service_token_survives_a_reload() {
        let dir = std::env::temp_dir().join("hippocampus-settings-test-cf-access-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        let state = SettingsState::load(path.clone());
        let mut settings = state.snapshot();
        settings.cf_access_client_id = Some("abc123.access".to_string());
        settings.cf_access_client_secret = Some("shh".to_string());
        state.persist(&settings).unwrap();

        let reloaded = SettingsState::load(path.clone()).snapshot();
        assert_eq!(
            reloaded.cf_access_client_id,
            Some("abc123.access".to_string())
        );
        assert_eq!(reloaded.cf_access_client_secret, Some("shh".to_string()));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_settings_file_without_cf_access_fields_loads_as_the_default() {
        let dir = std::env::temp_dir().join("hippocampus-settings-test-cf-access-absent");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, br#"{"capture_shortcut":"Super+Shift+KeyH"}"#).unwrap();

        let settings = SettingsState::load(path.clone()).snapshot();
        assert_eq!(settings.cf_access_client_id, None);
        assert_eq!(settings.cf_access_client_secret, None);

        std::fs::remove_file(&path).ok();
    }
}
