//! The one secret this app holds, kept where macOS keeps secrets.
//!
//! The Cloudflare Access Service Token secret used to sit in
//! `settings.json` as plain text, next to the shortcut and the backend
//! URL. Those two are preferences; this one is a credential that opens
//! every capture ever recorded to whoever holds it — it belongs in the
//! Keychain, behind the login password, out of Time Machine's plain
//! reach, and out of any log or crash dump that happens to include the
//! config file.
//!
//! There is deliberately no fallback to a file. If the Keychain cannot be
//! reached, saving fails and says so: a silent downgrade to plain text is
//! exactly the failure this module exists to prevent.

#[cfg(not(target_os = "ios"))]
use keyring::{Entry, Error};
#[cfg(target_os = "ios")]
use keyring_core::{Entry, Error};

/// Matches the bundle identifier, so the entry is recognisable in
/// Keychain Access rather than looking like something that wandered in.
const SERVICE: &str = "com.andreasbauer.hippocampus";

/// There is exactly one of these, and naming it after what it is beats
/// naming it after the user — the Client ID is not a secret and lives in
/// `settings.json`, so it would only be a second copy to keep in sync.
const ACCOUNT: &str = "cf-access-service-token";

#[cfg(not(target_os = "ios"))]
fn entry() -> Result<Entry, Error> {
    Entry::new(SERVICE, ACCOUNT)
}

/// On iOS the item goes into the app's own data-protection keychain,
/// readable once the phone has been unlocked. Whether it outlives a
/// renewal of a free signature is one of the things spike a) in
/// `docs/iphone.md` is there to find out: the keychain access group is
/// derived from the signing team, not from the app.
#[cfg(target_os = "ios")]
fn entry() -> Result<Entry, Error> {
    static STORE: std::sync::Once = std::sync::Once::new();
    STORE.call_once(
        || match apple_native_keyring_store::protected::Store::new() {
            Ok(store) => keyring_core::set_default_store(store),
            Err(err) => log::error!("could not open the keychain: {err}"),
        },
    );
    Entry::new(SERVICE, ACCOUNT)
}

/// Writes the secret, replacing whatever was there.
pub fn store(secret: &str) -> anyhow::Result<()> {
    entry()?.set_password(secret)?;
    Ok(())
}

/// Reads the secret, or `None` when none was ever stored.
///
/// A missing entry is an ordinary state — a fresh install, or a backend
/// that never sat behind Access — so it is not an error. Anything else is:
/// a locked or unavailable Keychain must not look the same as "no
/// credentials", or the app would silently start making unauthenticated
/// requests and blame the backend for refusing them.
pub fn read() -> anyhow::Result<Option<String>> {
    match entry()?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Removes the secret. Succeeds when there was nothing to remove.
pub fn clear() -> anyhow::Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real Keychain, so it is ignored by default: on a
    /// developer's Mac it can pop an authorisation dialog, and in CI
    /// there is no Keychain at all. Run it deliberately with
    /// `cargo test -- --ignored keychain`.
    #[test]
    #[ignore = "touches the real Keychain"]
    fn a_secret_round_trips_and_can_be_removed() {
        store("shh").unwrap();
        assert_eq!(read().unwrap(), Some("shh".to_string()));
        clear().unwrap();
        assert_eq!(read().unwrap(), None);
        // Clearing again is not an error — that is what makes it safe to
        // call on every "clear the credentials" action.
        clear().unwrap();
    }
}
