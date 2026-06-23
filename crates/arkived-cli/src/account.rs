//! Saved-account management.
//!
//! Lets a user attach a connection under a friendly name, switch between saved
//! accounts, and have plain commands (`ls`, `cat`, …) use the active one with
//! no credential flags. Account *metadata* (name, endpoint) lives in the local
//! SQLite [`Store`]; the *secret* connection parts live only in the OS keychain
//! via [`OsKeyring`] — never in plaintext on disk.
//!
//! The functions take the `Store` and a `&dyn CredentialStore` so they can be
//! unit-tested with an in-memory store and a fake secret store.

use anyhow::{bail, Context, Result};
use arkived_core::{
    ConnectionParts, CredentialStore, CurrentContext, OsKeyring, StorageAccount, Store,
};
use secrecy::{ExposeSecret, SecretString};
use std::path::PathBuf;

/// Keychain service namespace for all arkived secrets.
const KEYRING_SERVICE: &str = "arkived";

/// Keychain key under which a saved account's [`ConnectionParts`] JSON lives.
fn secret_key(name: &str) -> String {
    format!("account:{name}")
}

/// The OS-keychain secret store used for saved-account credentials.
pub fn keyring() -> OsKeyring {
    OsKeyring::new(KEYRING_SERVICE)
}

/// Resolve the path of the CLI's state database.
///
/// Honors `ARKIVED_STORE_PATH` first (used by tests and power users), else a
/// per-platform data directory: `%APPDATA%` (Windows), `XDG_DATA_HOME` or
/// `~/.local/share` (Linux), `~/Library/Application Support` (macOS).
pub fn default_store_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ARKIVED_STORE_PATH") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let base = data_dir().context("could not determine a data directory for the arkived store")?;
    Ok(base.join("arkived").join("arkived-state.sqlite3"))
}

#[cfg(target_os = "windows")]
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

/// Open (creating parent dirs as needed) the CLI's state database.
pub fn open_store() -> Result<Store> {
    let path = default_store_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating store directory {}", parent.display()))?;
    }
    Store::open(&path).with_context(|| format!("opening store at {}", path.display()))
}

/// Save `parts` under `name`, replacing any existing account of that name.
pub fn add(
    store: &Store,
    secrets: &dyn CredentialStore,
    name: &str,
    parts: &ConnectionParts,
) -> Result<()> {
    if parts.is_empty() {
        bail!(
            "no connection provided to save. Pass credentials alongside `account add`, e.g.\n  \
             arkived --connection-string \"...\" account add {name}"
        );
    }
    let json = serde_json::to_string(parts).context("serializing connection parts")?;
    secrets
        .put(&secret_key(name), &SecretString::new(json))
        .context("writing account secret to keychain")?;

    let (_acct_name, endpoint) = parts.summary();
    let record = StorageAccount {
        name: name.to_string(),
        subscription_id: None,
        kind: "attached".into(),
        region: String::new(),
        replication: String::new(),
        tier: String::new(),
        hns: false,
        endpoint: endpoint.unwrap_or_default(),
        attached_directly: true,
    };
    store
        .storage_account_upsert(&record)
        .context("saving account metadata")?;
    Ok(())
}

/// List all saved accounts.
pub fn list(store: &Store) -> Result<Vec<StorageAccount>> {
    store.storage_account_list_all().map_err(Into::into)
}

/// Make `name` the active account for subsequent commands.
pub fn use_account(store: &Store, name: &str) -> Result<()> {
    if store.storage_account_get(name)?.is_none() {
        bail!("no saved account named '{name}' (see `arkived account list`)");
    }
    store.context_set_account(Some(name))?;
    Ok(())
}

/// The current selection context (active sign-in / subscription / account).
pub fn current(store: &Store) -> Result<CurrentContext> {
    store.context_get().map_err(Into::into)
}

/// Remove a saved account: its secret, its metadata, and any active selection.
pub fn forget(store: &Store, secrets: &dyn CredentialStore, name: &str) -> Result<()> {
    secrets.delete(&secret_key(name))?;
    store.storage_account_delete(name)?;
    if store.context_get()?.account_name.as_deref() == Some(name) {
        store.context_set_account(None)?;
    }
    Ok(())
}

/// Load the active saved account's [`ConnectionParts`], if one is selected.
///
/// Returns `Ok(None)` when no account is active. Used by the connection
/// resolver as a fallback when no explicit credential flags are supplied.
pub fn active_parts(
    store: &Store,
    secrets: &dyn CredentialStore,
) -> Result<Option<(String, ConnectionParts)>> {
    let Some(name) = store.context_get()?.account_name else {
        return Ok(None);
    };
    match secrets.get(&secret_key(&name)) {
        Ok(secret) => {
            let parts: ConnectionParts = serde_json::from_str(secret.expose_secret())
                .context("deserializing saved connection parts")?;
            Ok(Some((name, parts)))
        }
        Err(arkived_core::Error::NotFound { .. }) => {
            bail!("active account '{name}' has no stored credentials; re-add it with `account add`")
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory `CredentialStore` for tests (no OS keychain).
    struct FakeSecrets(Mutex<HashMap<String, String>>);
    impl FakeSecrets {
        fn new() -> Self {
            Self(Mutex::new(HashMap::new()))
        }
    }
    impl CredentialStore for FakeSecrets {
        fn put(&self, key: &str, secret: &SecretString) -> Result<(), arkived_core::Error> {
            self.0
                .lock()
                .unwrap()
                .insert(key.into(), secret.expose_secret().into());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<SecretString, arkived_core::Error> {
            self.0
                .lock()
                .unwrap()
                .get(key)
                .map(|s| SecretString::new(s.clone()))
                .ok_or_else(|| arkived_core::Error::NotFound {
                    resource: key.into(),
                })
        }
        fn delete(&self, key: &str) -> Result<(), arkived_core::Error> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn parts(account: &str) -> ConnectionParts {
        ConnectionParts {
            connection_string: Some(format!(
                "DefaultEndpointsProtocol=https;AccountName={account};AccountKey=dGVzdA==;\
                 EndpointSuffix=core.windows.net"
            )),
            ..Default::default()
        }
    }

    #[test]
    fn add_then_list_and_current() {
        let store = Store::open_in_memory().unwrap();
        let secrets = FakeSecrets::new();
        add(&store, &secrets, "prod", &parts("acme")).unwrap();

        let listed = list(&store).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "prod");
        assert!(listed[0].attached_directly);
        assert_eq!(listed[0].endpoint, "https://acme.blob.core.windows.net");

        // No account is active until `use`.
        assert_eq!(current(&store).unwrap().account_name, None);
    }

    #[test]
    fn use_unknown_account_errors() {
        let store = Store::open_in_memory().unwrap();
        assert!(use_account(&store, "ghost").is_err());
    }

    #[test]
    fn use_sets_active_and_active_parts_roundtrips() {
        let store = Store::open_in_memory().unwrap();
        let secrets = FakeSecrets::new();
        add(&store, &secrets, "prod", &parts("acme")).unwrap();
        use_account(&store, "prod").unwrap();

        assert_eq!(
            current(&store).unwrap().account_name.as_deref(),
            Some("prod")
        );
        let (name, loaded) = active_parts(&store, &secrets).unwrap().unwrap();
        assert_eq!(name, "prod");
        assert_eq!(loaded.summary().0.as_deref(), Some("acme"));
    }

    #[test]
    fn active_parts_none_when_nothing_selected() {
        let store = Store::open_in_memory().unwrap();
        let secrets = FakeSecrets::new();
        assert!(active_parts(&store, &secrets).unwrap().is_none());
    }

    #[test]
    fn forget_removes_secret_metadata_and_active_selection() {
        let store = Store::open_in_memory().unwrap();
        let secrets = FakeSecrets::new();
        add(&store, &secrets, "prod", &parts("acme")).unwrap();
        use_account(&store, "prod").unwrap();

        forget(&store, &secrets, "prod").unwrap();
        assert!(list(&store).unwrap().is_empty());
        assert_eq!(current(&store).unwrap().account_name, None);
        assert!(active_parts(&store, &secrets).unwrap().is_none());
    }

    #[test]
    fn add_empty_parts_is_rejected() {
        let store = Store::open_in_memory().unwrap();
        let secrets = FakeSecrets::new();
        assert!(add(&store, &secrets, "x", &ConnectionParts::default()).is_err());
    }
}
