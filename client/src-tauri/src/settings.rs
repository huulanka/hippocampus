//! Settings that outlive a single run of the app.
//!
//! Which key summons the capture field, stored as a Tauri accelerator
//! string ("Super+Shift+KeyH") rather than as a parsed shortcut, because
//! that is the form both the plugin and a human editing the file by hand
//! can read. Which backend the client talks to — `None` means the
//! built-in default, so a fresh install needs no configuration to work
//! against a locally-run backend. And, once that backend sits behind
//! Cloudflare Access rather than on localhost, the Service Token that
//! gets it past Access without a browser login.
//!
//! That token is split deliberately. The Client ID is an identifier and
//! lives in `settings.json` with everything else; the Client Secret is a
//! credential and lives in the Keychain ([`crate::keychain`]) — it is
//! never written to disk here, and never handed to the webview, which has
//! no use for it now that requests are built on this side
//! ([`crate::backend`]).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::keychain;

/// Cmd+Shift+H, chosen because macOS leaves it alone: Cmd+Space and
/// Cmd+Shift+Space are Spotlight and input-source switching, and Option
/// combinations collide with text input on a German keyboard layout.
pub const DEFAULT_CAPTURE_SHORTCUT: &str = "Super+Shift+KeyH";

/// Where captures go when nothing else is configured. The environment
/// variable is the development override; the settings file wins over
/// both, because it is the one a user can actually reach.
const DEFAULT_BACKEND_URL: &str = "http://localhost:8080";
const BACKEND_URL_ENV: &str = "HIPPOCAMPUS_API_BASE_URL";

const FILE_NAME: &str = "settings.json";

/// What `settings.json` holds. Note what is *not* here: the Access Client
/// Secret. See [`Stored::legacy_secret`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stored {
    pub capture_shortcut: String,
    /// `None` means "use the built-in default", not "unset" — keeps a
    /// `settings.json` from an older version (which has no such field)
    /// loading as if the user had never touched this.
    #[serde(default)]
    pub backend_url: Option<String>,
    /// The Service Token's Client ID. Not a secret: it identifies the
    /// token, it does not authenticate it, and Cloudflare shows it in the
    /// dashboard next to the application.
    #[serde(default)]
    pub cf_access_client_id: Option<String>,
    /// Only ever *read*, never written — `skip_serializing` is what makes
    /// that true. Versions up to 1.2.0 stored the Client Secret here in
    /// plain text; [`SettingsState::load`] moves any it finds into the
    /// Keychain and rewrites the file without it, so the plaintext copy
    /// disappears the first time this version starts.
    #[serde(default, rename = "cf_access_client_secret", skip_serializing)]
    pub legacy_secret: Option<String>,
    /// Whether reading is behind the machine's own authentication
    /// ([`crate::lock`]). `None` means "not decided yet", which is read
    /// as on — a settings file written before this existed belongs to
    /// someone who has never been asked, and the safe reading of silence
    /// about a guard is that they want it. It makes itself inert anyway
    /// on a machine that cannot authenticate at all.
    #[serde(default)]
    pub lock_enabled: Option<bool>,
    /// How long the app may sit unattended before locking itself again.
    #[serde(default)]
    pub lock_idle_seconds: Option<u64>,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            capture_shortcut: DEFAULT_CAPTURE_SHORTCUT.to_string(),
            backend_url: None,
            cf_access_client_id: None,
            legacy_secret: None,
            lock_enabled: None,
            lock_idle_seconds: None,
        }
    }
}

/// What the webview is told. The secret is represented by a single bool:
/// enough to render "a secret is saved" and offer to replace it, useless
/// to anything that might read the webview's memory or a log line.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub capture_shortcut: String,
    pub backend_url: Option<String>,
    pub cf_access_client_id: Option<String>,
    pub cf_access_configured: bool,
    pub lock: crate::lock::LockStatus,
}

/// The Keychain read is lazy, so a local or LAN install — which never
/// needs a Service Token — never triggers a Keychain authorisation
/// dialog at all.
enum SecretCache {
    Unread,
    Known(Option<String>),
}

pub struct SettingsState {
    current: Mutex<Stored>,
    secret: Mutex<SecretCache>,
    path: PathBuf,
}

impl SettingsState {
    /// Reads the settings file, falling back to defaults, and migrates a
    /// plaintext secret left behind by an older version.
    ///
    /// A corrupt or unreadable file is reported and then ignored rather
    /// than being allowed to stop the app: losing a preference is a small
    /// annoyance, not being able to capture at all is not.
    pub fn load(path: PathBuf) -> Self {
        let stored = match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<Stored>(&raw).unwrap_or_else(|err| {
                log::warn!("settings file at {} is unreadable: {err}", path.display());
                Stored::default()
            }),
            Err(_) => Stored::default(),
        };

        let state = Self {
            current: Mutex::new(stored),
            secret: Mutex::new(SecretCache::Unread),
            path,
        };
        state.migrate_plaintext_secret();
        state
    }

    /// Moves a secret found in `settings.json` into the Keychain and
    /// rewrites the file without it.
    ///
    /// If the Keychain refuses, the file is left exactly as it is and the
    /// failure is logged as an error: throwing the only copy of a working
    /// credential away would be worse than leaving it where it already
    /// was, and the user is told loudly enough to act.
    fn migrate_plaintext_secret(&self) {
        let Some(secret) = self
            .current
            .lock()
            .ok()
            .and_then(|stored| stored.legacy_secret.clone())
        else {
            return;
        };

        if let Err(err) = keychain::store(&secret) {
            log::error!(
                "the Cloudflare Access secret is still in plain text in {}: \
                 the Keychain could not be written ({err})",
                self.path.display()
            );
            // Usable this session even so — the request path reads it
            // through the same cache either way.
            if let Ok(mut cache) = self.secret.lock() {
                *cache = SecretCache::Known(Some(secret));
            }
            return;
        }

        let rewritten = {
            let Ok(mut stored) = self.current.lock() else {
                return;
            };
            stored.legacy_secret = None;
            stored.clone()
        };

        if let Err(err) = self.persist(&rewritten) {
            log::error!(
                "the Cloudflare Access secret is now in the Keychain, but the \
                 plaintext copy in {} could not be removed ({err}) — delete it by hand",
                self.path.display()
            );
            return;
        }

        if let Ok(mut cache) = self.secret.lock() {
            *cache = SecretCache::Known(Some(secret));
        }
        log::info!("moved the Cloudflare Access secret out of settings.json into the Keychain");
    }

    pub fn snapshot(&self) -> Stored {
        self.current
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    pub fn view(&self, lock: &crate::lock::LockState) -> SettingsView {
        let stored = self.snapshot();
        SettingsView {
            capture_shortcut: stored.capture_shortcut,
            backend_url: stored.backend_url,
            cf_access_client_id: stored.cf_access_client_id,
            cf_access_configured: self.secret().is_some(),
            lock: self.lock_status(lock),
        }
    }

    /// Whether the guard is armed: switched on *and* on a machine that
    /// can actually authenticate. The second half is the escape hatch —
    /// without it, a Mac whose Touch ID has stopped working would hold
    /// its owner out of their own notes with no way back in.
    pub fn lock_armed(&self) -> bool {
        crate::lock::armed(self.snapshot().lock_enabled, crate::lock::mechanism())
    }

    pub fn lock_idle(&self) -> std::time::Duration {
        std::time::Duration::from_secs(
            self.snapshot()
                .lock_idle_seconds
                .unwrap_or(crate::lock::DEFAULT_IDLE_SECONDS)
                .clamp(crate::lock::MIN_IDLE_SECONDS, crate::lock::MAX_IDLE_SECONDS),
        )
    }

    pub fn lock_status(&self, lock: &crate::lock::LockState) -> crate::lock::LockStatus {
        let mechanism = crate::lock::mechanism();
        let armed = crate::lock::armed(self.snapshot().lock_enabled, mechanism);
        crate::lock::LockStatus {
            mechanism,
            enabled: self.snapshot().lock_enabled.unwrap_or(true),
            locked: armed && !lock.is_unlocked(),
            idle_seconds: self.lock_idle().as_secs(),
        }
    }

    /// The stored secret, read from the Keychain at most once per run.
    pub fn secret(&self) -> Option<String> {
        let mut cache = self.secret.lock().ok()?;
        if let SecretCache::Unread = *cache {
            let read = keychain::read().unwrap_or_else(|err| {
                log::error!("could not read the Cloudflare Access secret: {err}");
                None
            });
            *cache = SecretCache::Known(read);
        }
        match &*cache {
            SecretCache::Known(secret) => secret.clone(),
            SecretCache::Unread => None,
        }
    }

    fn remember_secret(&self, secret: Option<String>) {
        if let Ok(mut cache) = self.secret.lock() {
            *cache = SecretCache::Known(secret);
        }
    }

    /// The Service Token to send, or `None` when the backend is not
    /// behind Access. Both halves or neither: a request carrying an ID
    /// with no secret is not half-authenticated, it is unauthenticated
    /// and misleading about it.
    pub fn credentials(&self) -> Option<(String, String)> {
        let id = self.snapshot().cf_access_client_id?;
        Some((id, self.secret()?))
    }

    /// Which backend to talk to, without a trailing slash. Settings first,
    /// then the environment override, then the built-in default.
    pub fn backend_base(&self) -> String {
        let configured = self.snapshot().backend_url.filter(|url| !url.is_empty());
        let url = configured
            .or_else(|| {
                std::env::var(BACKEND_URL_ENV)
                    .ok()
                    .filter(|v| !v.is_empty())
            })
            .unwrap_or_else(|| DEFAULT_BACKEND_URL.to_string());
        url.trim_end_matches('/').to_string()
    }

    /// The shortcut as the plugin wants it, or the default if what is
    /// stored no longer parses.
    pub fn capture_shortcut(&self) -> Shortcut {
        parse(&self.snapshot().capture_shortcut).unwrap_or_else(|_| {
            parse(DEFAULT_CAPTURE_SHORTCUT).expect("the default shortcut must parse")
        })
    }

    fn persist(&self, settings: &Stored) -> anyhow::Result<()> {
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
pub fn get_settings(
    state: tauri::State<'_, SettingsState>,
    lock: tauri::State<'_, crate::lock::LockState>,
) -> SettingsView {
    state.view(&lock)
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
    lock: tauri::State<'_, crate::lock::LockState>,
    accelerator: String,
) -> Result<SettingsView, String> {
    let wanted = parse(&accelerator).map_err(|_| format!("{accelerator} is not a shortcut"))?;
    let previous = state.capture_shortcut();

    if wanted == previous {
        return Ok(state.view(&lock));
    }

    let shortcuts = app.global_shortcut();
    shortcuts.register(wanted).map_err(|err| {
        format!("that combination could not be registered — something else may own it ({err})")
    })?;
    if let Err(err) = shortcuts.unregister(previous) {
        log::warn!("could not release the previous shortcut: {err}");
    }

    let updated = Stored {
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

    Ok(state.view(&lock))
}

/// Points the client at a different backend, or back at the built-in
/// default when given an empty string.
///
/// No reachability check happens here — the caller does that through
/// `backend::check_backend` before committing to a value, so a typo does
/// not lock the user out of the settings screen that would let them fix
/// it.
#[tauri::command]
pub fn set_backend_url(
    state: tauri::State<'_, SettingsState>,
    lock: tauri::State<'_, crate::lock::LockState>,
    url: Option<String>,
) -> Result<SettingsView, String> {
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

    Ok(state.view(&lock))
}

/// Sets or clears the Cloudflare Access Service Token.
///
/// The ID goes to `settings.json`, the secret to the Keychain. An empty
/// ID clears both — an ID with no secret is not a state the client should
/// ever be in. An empty *secret* with an ID present means "keep the one
/// already stored", which is what lets the settings screen show the ID
/// without ever holding the secret it belongs to.
///
/// Unlike the preference writers above, a failure here is returned rather
/// than logged: a credential the user believes is saved and is not would
/// surface later as an unexplained 403.
#[tauri::command]
pub fn set_cf_access_credentials(
    state: tauri::State<'_, SettingsState>,
    lock: tauri::State<'_, crate::lock::LockState>,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<SettingsView, String> {
    let id = client_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let secret = client_secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let Some(id) = id else {
        keychain::clear().map_err(|err| format!("could not clear the Keychain entry: {err}"))?;
        state.remember_secret(None);
        let updated = {
            let mut current = state
                .current
                .lock()
                .map_err(|_| "settings lock poisoned".to_string())?;
            current.cf_access_client_id = None;
            current.clone()
        };
        if let Err(err) = state.persist(&updated) {
            log::warn!("could not write settings: {err}");
        }
        return Ok(state.view(&lock));
    };

    match secret {
        Some(secret) => {
            keychain::store(&secret)
                .map_err(|err| format!("could not write to the Keychain: {err}"))?;
            state.remember_secret(Some(secret));
        }
        None if state.secret().is_some() => {}
        None => return Err("need a Client Secret — none is stored yet".to_string()),
    }

    let updated = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        current.cf_access_client_id = Some(id);
        current.clone()
    };
    if let Err(err) = state.persist(&updated) {
        log::warn!("could not write settings: {err}");
    }

    Ok(state.view(&lock))
}

/// Switches the guard on or off.
///
/// Turning it **off** authenticates first, and that is the whole point:
/// otherwise the lock screen would have a button on it that removes the
/// lock, which is not a lock. Turning it on needs nothing — arming a
/// guard is not a privilege.
#[tauri::command]
pub async fn set_lock_enabled(
    state: tauri::State<'_, SettingsState>,
    lock: tauri::State<'_, crate::lock::LockState>,
    enabled: bool,
) -> Result<SettingsView, String> {
    if !enabled && state.lock_armed() {
        let granted = tauri::async_runtime::spawn_blocking(crate::lock::authenticate)
            .await
            .map_err(|err| format!("the authentication never finished: {err}"))??;
        if !granted {
            return Err("not unlocked, so the guard stays on".to_string());
        }
        // Switched off means nothing is guarded, so leave it open rather
        // than in a locked state nothing would ever reopen.
        lock.unlock();
    }

    let updated = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        current.lock_enabled = Some(enabled);
        current.clone()
    };
    if let Err(err) = state.persist(&updated) {
        log::warn!("could not write settings: {err}");
    }

    Ok(state.view(&lock))
}

/// How long the app may sit unattended before locking. Clamped rather
/// than rejected: a value out of range is a slider that went too far, not
/// something worth refusing.
#[tauri::command]
pub fn set_lock_idle_seconds(
    state: tauri::State<'_, SettingsState>,
    lock: tauri::State<'_, crate::lock::LockState>,
    seconds: u64,
) -> Result<SettingsView, String> {
    let clamped = seconds.clamp(crate::lock::MIN_IDLE_SECONDS, crate::lock::MAX_IDLE_SECONDS);

    let updated = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        current.lock_idle_seconds = Some(clamped);
        current.clone()
    };
    if let Err(err) = state.persist(&updated) {
        log::warn!("could not write settings: {err}");
    }

    Ok(state.view(&lock))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hippocampus-settings-test-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

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
        let path = temp_path("broken");
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
        let path = temp_path("backend-url-absent");
        std::fs::write(&path, br#"{"capture_shortcut":"Super+Shift+KeyH"}"#).unwrap();

        let state = SettingsState::load(path.clone());
        assert_eq!(state.snapshot().backend_url, None);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_saved_backend_url_survives_a_reload() {
        let path = temp_path("backend-url-roundtrip");

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

    /// The Client ID is an identifier, so it stays in the file.
    #[test]
    fn the_client_id_survives_a_reload() {
        let path = temp_path("cf-id-roundtrip");

        let state = SettingsState::load(path.clone());
        let mut settings = state.snapshot();
        settings.cf_access_client_id = Some("abc123.access".to_string());
        state.persist(&settings).unwrap();

        let reloaded = SettingsState::load(path.clone()).snapshot();
        assert_eq!(
            reloaded.cf_access_client_id,
            Some("abc123.access".to_string())
        );

        std::fs::remove_file(&path).ok();
    }

    /// The point of the split: whatever else `persist` writes, the secret
    /// is not part of it. This is the regression test for the plaintext
    /// storage this version removed.
    #[test]
    fn the_secret_is_never_written_to_the_settings_file() {
        let path = temp_path("no-plaintext-secret");

        let state = SettingsState::load(path.clone());
        let settings = Stored {
            cf_access_client_id: Some("abc123.access".to_string()),
            // As if it had just been read from an older file.
            legacy_secret: Some("super-secret-value".to_string()),
            ..state.snapshot()
        };
        state.persist(&settings).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains("super-secret-value"));
        assert!(!written.contains("cf_access_client_secret"));
        assert!(written.contains("abc123.access"));

        std::fs::remove_file(&path).ok();
    }

    /// An older file's plaintext secret is still *readable*, because that
    /// is what the migration needs in order to move it.
    #[test]
    fn an_older_file_with_a_plaintext_secret_still_parses() {
        let raw = br#"{"capture_shortcut":"Super+Shift+KeyH","cf_access_client_id":"abc","cf_access_client_secret":"shh"}"#;
        let stored: Stored = serde_json::from_slice(raw).unwrap();
        assert_eq!(stored.legacy_secret, Some("shh".to_string()));
    }

    #[test]
    fn a_settings_file_without_cf_access_fields_loads_as_the_default() {
        let path = temp_path("cf-access-absent");
        std::fs::write(&path, br#"{"capture_shortcut":"Super+Shift+KeyH"}"#).unwrap();

        let settings = SettingsState::load(path.clone()).snapshot();
        assert_eq!(settings.cf_access_client_id, None);
        assert_eq!(settings.legacy_secret, None);

        std::fs::remove_file(&path).ok();
    }

    /// Settings beat the environment; the environment beats the built-in
    /// default. The trailing slash is dropped either way, because every
    /// caller appends a path beginning with one.
    #[test]
    fn the_backend_base_prefers_the_setting_and_drops_a_trailing_slash() {
        let path = temp_path("backend-base");
        let state = SettingsState::load(path.clone());

        assert_eq!(state.backend_base(), DEFAULT_BACKEND_URL);

        let settings = Stored {
            backend_url: Some("https://hippocampus.example.com/".to_string()),
            ..state.snapshot()
        };
        *state.current.lock().unwrap() = settings;
        assert_eq!(state.backend_base(), "https://hippocampus.example.com");

        std::fs::remove_file(&path).ok();
    }

    /// Without a Client ID there is nothing to authenticate with, no
    /// matter what the Keychain holds — so the request goes out plain
    /// rather than half-signed.
    #[test]
    fn credentials_need_both_halves() {
        let path = temp_path("credentials-need-both");
        let state = SettingsState::load(path.clone());
        state.remember_secret(Some("shh".to_string()));

        assert_eq!(state.credentials(), None);

        *state.current.lock().unwrap() = Stored {
            cf_access_client_id: Some("abc".to_string()),
            ..state.snapshot()
        };
        assert_eq!(
            state.credentials(),
            Some(("abc".to_string(), "shh".to_string()))
        );

        std::fs::remove_file(&path).ok();
    }
}
