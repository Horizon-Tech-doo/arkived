//! Current-context 3-tuple (sign_in, subscription, account) persisted in
//! the `context` key-value table.

use crate::store::Store;
use crate::Error;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// The currently-selected context. All three fields are independent; any may
/// be `None`. CLI write commands require `account_name` resolved via `-A`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentContext {
    /// The current Entra sign-in, or `None` if no sign-in active.
    pub sign_in_id: Option<String>,
    /// The current subscription within the sign-in, or `None`.
    pub subscription_id: Option<String>,
    /// The current storage account within the subscription, or `None`.
    pub account_name: Option<String>,
}

impl Store {
    /// Fetch the full current context.
    pub fn context_get(&self) -> Result<CurrentContext, Error> {
        self.with_conn(|c| {
            let get = |k: &str| -> Result<Option<String>, Error> {
                c.query_row("SELECT v FROM context WHERE k = ?1", params![k], |r| {
                    r.get::<_, Option<String>>(0)
                })
                .optional()
                .map(|opt| opt.flatten())
                .map_err(|e| Error::Other(anyhow::anyhow!("context get {k}: {e}")))
            };
            Ok(CurrentContext {
                sign_in_id: get("current_sign_in")?,
                subscription_id: get("current_subscription")?,
                account_name: get("current_account")?,
            })
        })
    }

    /// Set the current sign-in (or clear with `None`).
    pub fn context_set_sign_in(&self, id: Option<&str>) -> Result<(), Error> {
        self.context_set("current_sign_in", id)
    }

    /// Set the current subscription (or clear with `None`).
    pub fn context_set_subscription(&self, id: Option<&str>) -> Result<(), Error> {
        self.context_set("current_subscription", id)
    }

    /// Set the current storage account (or clear with `None`).
    pub fn context_set_account(&self, name: Option<&str>) -> Result<(), Error> {
        self.context_set("current_account", name)
    }

    fn context_set(&self, key: &str, value: Option<&str>) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO context (k, v) VALUES (?1, ?2)
                 ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                params![key, value],
            )
            .map_err(|e| Error::Other(anyhow::anyhow!("context set {key}: {e}")))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn default_context_is_empty() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.context_get().unwrap(), CurrentContext::default());
    }

    #[test]
    fn set_and_get_each_key() {
        let s = Store::open_in_memory().unwrap();
        s.context_set_sign_in(Some("si-1")).unwrap();
        s.context_set_subscription(Some("sub-1")).unwrap();
        s.context_set_account(Some("acmeprod")).unwrap();
        let ctx = s.context_get().unwrap();
        assert_eq!(ctx.sign_in_id.as_deref(), Some("si-1"));
        assert_eq!(ctx.subscription_id.as_deref(), Some("sub-1"));
        assert_eq!(ctx.account_name.as_deref(), Some("acmeprod"));
    }

    #[test]
    fn clearing_by_setting_none() {
        let s = Store::open_in_memory().unwrap();
        s.context_set_account(Some("acmeprod")).unwrap();
        s.context_set_account(None).unwrap();
        assert_eq!(s.context_get().unwrap().account_name, None);
    }

    #[test]
    fn update_overwrites() {
        let s = Store::open_in_memory().unwrap();
        s.context_set_sign_in(Some("si-1")).unwrap();
        s.context_set_sign_in(Some("si-2")).unwrap();
        assert_eq!(s.context_get().unwrap().sign_in_id.as_deref(), Some("si-2"));
    }
}
