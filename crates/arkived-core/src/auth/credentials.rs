//! `CredentialStore` trait + OS-keychain-backed impl.

use crate::Error;
use secrecy::{ExposeSecret, SecretString};

/// Persists per-connection secrets in the OS keychain. No plaintext on disk.
pub trait CredentialStore: Send + Sync {
    /// Store a secret under `key`.
    fn put(&self, key: &str, secret: &SecretString) -> Result<(), Error>;
    /// Retrieve a secret previously stored under `key`.
    fn get(&self, key: &str) -> Result<SecretString, Error>;
    /// Remove a secret. Missing keys are not an error (idempotent).
    fn delete(&self, key: &str) -> Result<(), Error>;
}

/// OS-native keychain impl via the `keyring` crate.
/// - macOS: Keychain
/// - Windows: Credential Manager
/// - Linux: Secret Service (gnome-keyring/KWallet)
pub struct OsKeyring {
    service: String,
}

impl OsKeyring {
    /// Create a new keyring scoped to `service`. All keys are namespaced by this.
    pub fn new(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, Error> {
        keyring::Entry::new(&self.service, key)
            .map_err(|e| Error::AuthFailed(format!("keyring entry: {e}")))
    }
}

impl CredentialStore for OsKeyring {
    fn put(&self, key: &str, secret: &SecretString) -> Result<(), Error> {
        self.entry(key)?
            .set_password(secret.expose_secret())
            .map_err(|e| Error::AuthFailed(format!("keyring put: {e}")))
    }

    fn get(&self, key: &str) -> Result<SecretString, Error> {
        self.entry(key)?
            .get_password()
            .map(SecretString::new)
            .map_err(|e| match e {
                keyring::Error::NoEntry => Error::NotFound { resource: format!("keychain:{key}") },
                other => Error::AuthFailed(format!("keyring get: {other}")),
            })
    }

    fn delete(&self, key: &str) -> Result<(), Error> {
        match self.entry(key)?.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::AuthFailed(format!("keyring delete: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests hit the real OS keychain; they're marked #[ignore] so CI
    // can skip them on headless runners. Run locally with:
    //   cargo test -p arkived-core --lib auth::credentials::tests -- --ignored
    #[test]
    #[ignore]
    fn roundtrip_against_os_keychain() {
        let store = OsKeyring::new("arkived-test");
        let key = format!("test-{}", uuid::Uuid::new_v4());
        let secret = SecretString::new("super-secret".into());

        assert!(matches!(store.get(&key), Err(Error::NotFound { .. })));

        store.put(&key, &secret).unwrap();
        let got = store.get(&key).unwrap();
        assert_eq!(got.expose_secret(), "super-secret");

        store.delete(&key).unwrap();
        store.delete(&key).unwrap();
        assert!(matches!(store.get(&key), Err(Error::NotFound { .. })));
    }

    // Non-ignored test exercises the types without touching the OS.
    #[test]
    fn credential_store_is_object_safe() {
        fn assert_object_safe(_: &dyn CredentialStore) {}
        let store = OsKeyring::new("arkived-compile-check");
        assert_object_safe(&store);
    }
}
