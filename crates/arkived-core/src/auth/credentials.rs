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

const CHUNK_MARKER_PREFIX: &str = "arkived-chunked-v1:";
const MAX_KEYRING_CHUNK_BYTES: usize = 1800;

impl OsKeyring {
    /// Create a new keyring scoped to `service`. All keys are namespaced by this.
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, Error> {
        keyring::Entry::new(&self.service, key)
            .map_err(|e| Error::AuthFailed(format!("keyring entry: {e}")))
    }

    fn chunk_key(key: &str, index: usize) -> String {
        format!("{key}::chunk::{index}")
    }

    fn raw_put(&self, key: &str, secret: &str) -> Result<(), Error> {
        self.entry(key)?
            .set_password(secret)
            .map_err(|e| Error::AuthFailed(format!("keyring put: {e}")))
    }

    fn raw_get(&self, key: &str) -> Result<String, Error> {
        self.entry(key)?.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => Error::NotFound {
                resource: format!("keychain:{key}"),
            },
            other => Error::AuthFailed(format!("keyring get: {other}")),
        })
    }

    fn raw_delete(&self, key: &str) -> Result<(), Error> {
        match self.entry(key)?.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::AuthFailed(format!("keyring delete: {e}"))),
        }
    }

    fn parse_chunk_marker(value: &str) -> Option<usize> {
        value
            .strip_prefix(CHUNK_MARKER_PREFIX)
            .and_then(|count| count.parse::<usize>().ok())
    }

    fn delete_existing_chunks(&self, key: &str) -> Result<(), Error> {
        let existing = match self.raw_get(key) {
            Ok(existing) => existing,
            Err(Error::NotFound { .. }) => return Ok(()),
            Err(err) => return Err(err),
        };
        let Some(count) = Self::parse_chunk_marker(&existing) else {
            return Ok(());
        };
        for index in 0..count {
            self.raw_delete(&Self::chunk_key(key, index))?;
        }
        Ok(())
    }
}

impl CredentialStore for OsKeyring {
    fn put(&self, key: &str, secret: &SecretString) -> Result<(), Error> {
        let value = secret.expose_secret();
        self.delete_existing_chunks(key)?;

        if value.len() <= MAX_KEYRING_CHUNK_BYTES && !value.starts_with(CHUNK_MARKER_PREFIX) {
            return self.raw_put(key, value);
        }

        let chunks = chunk_secret(value);
        for (index, chunk) in chunks.iter().enumerate() {
            self.raw_put(&Self::chunk_key(key, index), chunk)?;
        }
        self.raw_put(key, &format!("{CHUNK_MARKER_PREFIX}{}", chunks.len()))
    }

    fn get(&self, key: &str) -> Result<SecretString, Error> {
        let value = self.raw_get(key)?;
        let Some(count) = Self::parse_chunk_marker(&value) else {
            return Ok(SecretString::new(value));
        };

        let mut joined = String::new();
        for index in 0..count {
            joined.push_str(&self.raw_get(&Self::chunk_key(key, index))?);
        }
        Ok(SecretString::new(joined))
    }

    fn delete(&self, key: &str) -> Result<(), Error> {
        self.delete_existing_chunks(key)?;
        self.raw_delete(key)
    }
}

fn chunk_secret(value: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if !current.is_empty() && current.len() + ch.len_utf8() > MAX_KEYRING_CHUNK_BYTES {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }

    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
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

    #[test]
    fn chunk_secret_splits_large_values_without_losing_bytes() {
        let source = "x".repeat(MAX_KEYRING_CHUNK_BYTES + 17);
        let chunks = chunk_secret(&source);
        assert_eq!(chunks.len(), 2);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.len() <= MAX_KEYRING_CHUNK_BYTES));
        assert_eq!(chunks.concat(), source);
    }

    #[test]
    fn chunk_secret_preserves_utf8_boundaries() {
        let source = format!("{}å", "x".repeat(MAX_KEYRING_CHUNK_BYTES));
        let chunks = chunk_secret(&source);
        assert_eq!(chunks.concat(), source);
    }

    #[test]
    fn chunk_marker_roundtrips_count() {
        assert_eq!(
            OsKeyring::parse_chunk_marker(&format!("{CHUNK_MARKER_PREFIX}3")),
            Some(3)
        );
        assert_eq!(OsKeyring::parse_chunk_marker("plain-secret"), None);
    }
}
