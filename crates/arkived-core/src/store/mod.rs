//! SQLite-backed state store. Holds connection metadata (sign-ins,
//! subscriptions, storage accounts, attached resources, current context,
//! session policy memory). Holds **no credentials** — those live in the
//! `CredentialStore` (OS keychain).
//!
//! Encryption-at-rest is a follow-up plan. v0.1.0 relies on OS file
//! permissions for metadata (secrets are in keychain).

use crate::Error;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

/// The schema version this build understands. Bumped on every migration.
pub(crate) const SCHEMA_VERSION: i32 = 1;

/// Handle to the SQLite-backed state store.
///
/// Cheap to clone via `Arc<Store>`. Internally guards the SQLite
/// connection behind a `Mutex`; concurrent callers serialize on the lock.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open (or create) a store at the given path, applying migrations.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let conn = Connection::open(path)
            .map_err(|e| Error::Other(anyhow::anyhow!("open store: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| Error::Other(anyhow::anyhow!("enable foreign_keys: {e}")))?;
        let store = Self { conn: Mutex::new(conn) };
        store.migrate()?;
        Ok(store)
    }

    /// Open an in-memory store — useful for tests.
    pub fn open_in_memory() -> Result<Self, Error> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Other(anyhow::anyhow!("open in-memory store: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| Error::Other(anyhow::anyhow!("enable foreign_keys: {e}")))?;
        let store = Self { conn: Mutex::new(conn) };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        ).map_err(|e| Error::Other(anyhow::anyhow!("migrate schema_meta: {e}")))?;

        let current: Option<i32> = conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'version'",
            [],
            |r| r.get::<_, String>(0).map(|s| s.parse::<i32>().unwrap_or(0)),
        ).ok();

        if current.unwrap_or(0) < 1 {
            conn.execute_batch(MIGRATION_V1)
                .map_err(|e| Error::Other(anyhow::anyhow!("apply v1 migration: {e}")))?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('version', ?1)",
                rusqlite::params![SCHEMA_VERSION.to_string()],
            ).map_err(|e| Error::Other(anyhow::anyhow!("record schema version: {e}")))?;
        }

        conn.execute("DELETE FROM policy_memory", [])
            .map_err(|e| Error::Other(anyhow::anyhow!("truncate policy_memory: {e}")))?;

        Ok(())
    }

    /// Access the underlying connection under a lock. Sub-modules use this.
    pub(crate) fn with_conn<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&Connection) -> Result<R, Error>,
    {
        let guard = self.conn.lock().unwrap();
        f(&guard)
    }

    /// Current schema version on disk (for diagnostics).
    pub fn schema_version(&self) -> i32 {
        self.with_conn(|c| {
            c.query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |r| {
                    let s: String = r.get(0)?;
                    Ok(s.parse::<i32>().unwrap_or(0))
                },
            ).map_err(|e| Error::Other(anyhow::anyhow!("read schema version: {e}")))
        }).unwrap_or(0)
    }
}

/// v1 migration: create all foundation tables.
const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS sign_in (
    id            TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL,
    tenant_id     TEXT NOT NULL,
    environment   TEXT NOT NULL,
    user_principal TEXT NOT NULL,
    added_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS subscription (
    id            TEXT PRIMARY KEY,
    sign_in_id    TEXT NOT NULL REFERENCES sign_in(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    tenant_id     TEXT NOT NULL,
    discovered_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS storage_account (
    name              TEXT PRIMARY KEY,
    subscription_id   TEXT REFERENCES subscription(id) ON DELETE SET NULL,
    kind              TEXT NOT NULL,
    region            TEXT NOT NULL,
    replication       TEXT NOT NULL,
    tier              TEXT NOT NULL,
    hns               INTEGER NOT NULL,
    endpoint          TEXT NOT NULL,
    attached_directly INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS attached_resource (
    id            TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    endpoint      TEXT NOT NULL,
    auth_kind     TEXT NOT NULL,
    keychain_ref  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS context (
    k TEXT PRIMARY KEY,
    v TEXT
);

CREATE TABLE IF NOT EXISTS policy_memory (
    action_kind TEXT NOT NULL,
    target      TEXT,
    allowed_at  TEXT NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_in_memory_applies_migrations() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version(), SCHEMA_VERSION);
    }

    #[test]
    fn open_on_disk_persists_schema() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");

        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.schema_version(), SCHEMA_VERSION);
        }

        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version(), SCHEMA_VERSION);
    }

    #[test]
    fn migration_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        store.migrate().unwrap();
        store.migrate().unwrap();
        assert_eq!(store.schema_version(), SCHEMA_VERSION);
    }

    use chrono::Utc;

    #[test]
    fn policy_memory_is_cleared_on_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");

        {
            let store = Store::open(&path).unwrap();
            store.with_conn(|c| {
                c.execute(
                    "INSERT INTO policy_memory (action_kind, target, allowed_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params!["delete_blob", "acme/x", Utc::now().to_rfc3339()],
                ).map_err(|e| Error::Other(anyhow::anyhow!(e)))?;
                Ok(())
            }).unwrap();

            let count: i64 = store.with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM policy_memory", [], |r| r.get(0))
                    .map_err(|e| Error::Other(anyhow::anyhow!(e)))
            }).unwrap();
            assert_eq!(count, 1);
        }

        let store = Store::open(&path).unwrap();
        let count: i64 = store.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM policy_memory", [], |r| r.get(0))
                .map_err(|e| Error::Other(anyhow::anyhow!(e)))
        }).unwrap();
        assert_eq!(count, 0, "policy_memory must be truncated on every open");
    }

    #[test]
    fn foreign_keys_enforced_by_default() {
        let s = Store::open_in_memory().unwrap();
        let enabled: i32 = s.with_conn(|c| {
            c.query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .map_err(|e| Error::Other(anyhow::anyhow!(e)))
        }).unwrap();
        assert_eq!(enabled, 1, "foreign_keys pragma must default to ON");
    }
}

pub mod sign_in;
pub use sign_in::SignIn;

pub mod subscription;
pub use subscription::Subscription;

pub mod storage_account;
pub use storage_account::StorageAccount;

pub mod attached_resource;
pub use attached_resource::AttachedResource;
