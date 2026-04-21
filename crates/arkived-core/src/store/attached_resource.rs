//! CRUD for the `attached_resource` table — resources attached directly via
//! SAS or connection string, outside a sign-in.

use crate::store::Store;
use crate::types::{AuthKind, ResourceKind};
use crate::Error;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// A resource attached via SAS URL, connection string, or account key —
/// tracked outside any Entra sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachedResource {
    /// Opaque caller-chosen id (typically a UUID).
    pub id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// What kind of resource this is.
    pub resource_kind: ResourceKind,
    /// Endpoint URL (with container/path if applicable).
    pub endpoint: String,
    /// Which auth method this attachment uses.
    pub auth_kind: AuthKind,
    /// Reference into the OS keychain for the actual secret.
    pub keychain_ref: String,
}

impl Store {
    /// Insert a new attached resource. Fails on duplicate `id`.
    pub fn attached_resource_insert(&self, a: &AttachedResource) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO attached_resource (id, display_name, resource_kind, endpoint, auth_kind, keychain_ref)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    a.id, a.display_name,
                    serde_json::to_string(&a.resource_kind).unwrap(),
                    a.endpoint,
                    serde_json::to_string(&a.auth_kind).unwrap(),
                    a.keychain_ref,
                ],
            ).map_err(|e| Error::Other(anyhow::anyhow!("attached_resource insert: {e}")))?;
            Ok(())
        })
    }

    /// Fetch an attached resource by id. Returns `Ok(None)` if not found.
    pub fn attached_resource_get(&self, id: &str) -> Result<Option<AttachedResource>, Error> {
        self.with_conn(|c| {
            c.query_row(
                "SELECT id, display_name, resource_kind, endpoint, auth_kind, keychain_ref
                 FROM attached_resource WHERE id = ?1",
                params![id],
                row_to_attached,
            )
            .optional()
            .map_err(|e| Error::Other(anyhow::anyhow!("attached_resource get: {e}")))
        })
    }

    /// List all attached resources, ordered by display name.
    pub fn attached_resource_list(&self) -> Result<Vec<AttachedResource>, Error> {
        self.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, display_name, resource_kind, endpoint, auth_kind, keychain_ref
                 FROM attached_resource ORDER BY display_name",
                )
                .map_err(|e| {
                    Error::Other(anyhow::anyhow!("attached_resource list prepare: {e}"))
                })?;
            let rows = stmt
                .query_map([], row_to_attached)
                .map_err(|e| Error::Other(anyhow::anyhow!("attached_resource list query: {e}")))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| {
                    Error::Other(anyhow::anyhow!("attached_resource list row: {e}"))
                })?);
            }
            Ok(out)
        })
    }

    /// Delete an attached resource by id. Does not remove the keychain entry.
    pub fn attached_resource_delete(&self, id: &str) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute("DELETE FROM attached_resource WHERE id = ?1", params![id])
                .map_err(|e| Error::Other(anyhow::anyhow!("attached_resource delete: {e}")))?;
            Ok(())
        })
    }
}

fn row_to_attached(row: &rusqlite::Row<'_>) -> rusqlite::Result<AttachedResource> {
    let rk: String = row.get(2)?;
    let ak: String = row.get(4)?;
    Ok(AttachedResource {
        id: row.get(0)?,
        display_name: row.get(1)?,
        resource_kind: serde_json::from_str(&rk).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
        endpoint: row.get(3)?,
        auth_kind: serde_json::from_str(&ak).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
        keychain_ref: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn sample() -> AttachedResource {
        AttachedResource {
            id: uuid::Uuid::new_v4().to_string(),
            display_name: "dev-readonly".into(),
            resource_kind: ResourceKind::BlobContainer,
            endpoint: "https://acmeprod.blob.core.windows.net/raw-telemetry".into(),
            auth_kind: AuthKind::SasToken,
            keychain_ref: "arkived:connection:abcd".into(),
        }
    }

    #[test]
    fn insert_get_list_delete() {
        let s = Store::open_in_memory().unwrap();
        let a = sample();
        s.attached_resource_insert(&a).unwrap();
        let got = s.attached_resource_get(&a.id).unwrap().unwrap();
        assert_eq!(got, a);

        let list = s.attached_resource_list().unwrap();
        assert_eq!(list.len(), 1);

        s.attached_resource_delete(&a.id).unwrap();
        assert!(s.attached_resource_get(&a.id).unwrap().is_none());
    }
}
