//! Keychain-backed cache for Entra refresh tokens.
//!
//! Refresh tokens are long-lived (up to 90 days by default). Persisting them
//! means users don't have to repeat device-code sign-in on every process start.
//! Access tokens are NOT cached — they're short-lived and produced on demand.

use crate::auth::credentials::CredentialStore;
use crate::Error;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// What the cache stores for a sign-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRefresh {
    /// Opaque refresh token.
    pub refresh_token: String,
    /// Tenant ID the token is scoped to.
    pub tenant: String,
    /// Client ID used to request the token.
    pub client_id: String,
    /// Scope granted on the most recent sign-in.
    pub scope: String,
    /// When the refresh was first obtained.
    #[serde(with = "time::serde::rfc3339")]
    pub obtained_at: OffsetDateTime,
}

/// Store/retrieve Entra refresh tokens via any `CredentialStore` impl.
pub struct RefreshCache<'a> {
    store: &'a dyn CredentialStore,
}

impl<'a> RefreshCache<'a> {
    /// Build a cache over a credential store.
    pub fn new(store: &'a dyn CredentialStore) -> Self {
        Self { store }
    }

    fn key_for(sign_in_id: &str) -> String {
        format!("arkived:entra-refresh:{sign_in_id}")
    }

    /// Persist a refresh token for a sign-in.
    pub fn put(&self, sign_in_id: &str, cached: &CachedRefresh) -> Result<(), Error> {
        let json = serde_json::to_string(cached)
            .map_err(|e| Error::Other(anyhow::anyhow!("serialize refresh cache: {e}")))?;
        self.store
            .put(&Self::key_for(sign_in_id), &SecretString::new(json.into()))
    }

    /// Retrieve a refresh token for a sign-in. Returns `None` if missing.
    pub fn get(&self, sign_in_id: &str) -> Result<Option<CachedRefresh>, Error> {
        match self.store.get(&Self::key_for(sign_in_id)) {
            Ok(secret) => {
                let cached: CachedRefresh = serde_json::from_str(secret.expose_secret())
                    .map_err(|e| Error::Other(anyhow::anyhow!("parse refresh cache: {e}")))?;
                Ok(Some(cached))
            }
            Err(Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Delete a refresh token from the cache.
    pub fn delete(&self, sign_in_id: &str) -> Result<(), Error> {
        self.store.delete(&Self::key_for(sign_in_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeStore(Mutex<HashMap<String, String>>);

    impl FakeStore {
        fn new() -> Self { Self(Mutex::new(HashMap::new())) }
    }

    impl CredentialStore for FakeStore {
        fn put(&self, key: &str, secret: &SecretString) -> Result<(), Error> {
            self.0.lock().unwrap().insert(key.into(), secret.expose_secret().into());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<SecretString, Error> {
            self.0
                .lock()
                .unwrap()
                .get(key)
                .map(|s| SecretString::new(s.clone().into()))
                .ok_or_else(|| Error::NotFound { resource: key.into() })
        }
        fn delete(&self, key: &str) -> Result<(), Error> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn sample() -> CachedRefresh {
        CachedRefresh {
            refresh_token: "RT-ABC".into(),
            tenant: "common".into(),
            client_id: "cid".into(),
            scope: "https://storage.azure.com/.default".into(),
            obtained_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn put_then_get_roundtrip() {
        let store = FakeStore::new();
        let cache = RefreshCache::new(&store);
        cache.put("si-1", &sample()).unwrap();
        let got = cache.get("si-1").unwrap().unwrap();
        assert_eq!(got.refresh_token, "RT-ABC");
        assert_eq!(got.tenant, "common");
    }

    #[test]
    fn missing_returns_none() {
        let store = FakeStore::new();
        let cache = RefreshCache::new(&store);
        assert!(cache.get("nope").unwrap().is_none());
    }

    #[test]
    fn delete_is_idempotent() {
        let store = FakeStore::new();
        let cache = RefreshCache::new(&store);
        cache.put("si-1", &sample()).unwrap();
        cache.delete("si-1").unwrap();
        cache.delete("si-1").unwrap();
        assert!(cache.get("si-1").unwrap().is_none());
    }
}
