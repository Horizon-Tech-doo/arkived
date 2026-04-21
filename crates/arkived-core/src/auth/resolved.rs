//! Resolved credential — the materialized output of `AuthProvider::resolve()`.
//!
//! Each variant is the form the storage backend needs for a particular auth
//! method. The backend pattern-matches on this enum to pick between Entra
//! OAuth, SharedKey HMAC signing, SAS URL appending, or anonymous access.

use crate::types::AuthKind;
use azure_core::credentials::TokenCredential;
use secrecy::SecretString;
use std::sync::Arc;

/// The credential form the backend will consume.
#[derive(Clone)]
pub enum ResolvedCredential {
    /// Microsoft Entra OAuth token credential (implements `TokenCredential`).
    Entra(Arc<dyn TokenCredential>),
    /// Storage shared key. Pipeline policy signs each request with HMAC-SHA256.
    SharedKey {
        /// Storage account name (canonicalized resource prefix).
        account_name: String,
        /// Base64-encoded key (as given by the user; decoded lazily by the signer).
        key: SecretString,
    },
    /// A SAS token. The backend appends this to every request URL.
    Sas(SecretString),
    /// No credential. Works only for public containers.
    Anonymous,
}

impl ResolvedCredential {
    /// Which `AuthKind` produced this (for logging).
    pub fn kind(&self) -> AuthKind {
        match self {
            Self::Entra(_) => AuthKind::EntraDeviceCode,
            Self::SharedKey { .. } => AuthKind::AccountKey,
            Self::Sas(_) => AuthKind::SasToken,
            Self::Anonymous => AuthKind::Anonymous,
        }
    }
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entra(_) => f.write_str("ResolvedCredential::Entra(***)"),
            Self::SharedKey { account_name, .. } => f
                .debug_struct("ResolvedCredential::SharedKey")
                .field("account_name", account_name)
                .field("key", &"***")
                .finish(),
            Self::Sas(_) => f.write_str("ResolvedCredential::Sas(***)"),
            Self::Anonymous => f.write_str("ResolvedCredential::Anonymous"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_secrets() {
        let r = ResolvedCredential::SharedKey {
            account_name: "acme".into(),
            key: SecretString::new("SUPER-SECRET-KEY".into()),
        };
        let dbg = format!("{r:?}");
        assert!(dbg.contains("acme"));
        assert!(!dbg.contains("SUPER-SECRET-KEY"));
        assert!(dbg.contains("***"));
    }

    #[test]
    fn debug_sas_hides_token() {
        let r = ResolvedCredential::Sas(SecretString::new("sv=2022&sig=SECRET".into()));
        let dbg = format!("{r:?}");
        assert!(!dbg.contains("SECRET"));
    }

    #[test]
    fn kind_mapping() {
        let a = ResolvedCredential::Anonymous;
        assert_eq!(a.kind(), AuthKind::Anonymous);
    }
}
