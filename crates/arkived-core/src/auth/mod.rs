//! `AuthProvider` trait and credential storage abstractions.
//!
//! Concrete impls (EntraDeviceCode, AccountKey, SasToken, etc.) land in
//! the Auth plan. This module only defines the contract.

pub mod credentials;
pub mod resolved;

use crate::types::{AuthKind, ResourceKind};
use async_trait::async_trait;
use std::sync::Arc;

/// An opaque credential factory consumed by the Azure SDK.
///
/// The return type is erased so every auth method can participate without
/// leaking SDK types into downstream code. Concrete `AuthProvider` impls
/// produce types implementing the Azure SDK's `TokenCredential`-equivalent
/// traits; for v0.1.0 we use a local trait that the backend will bridge.
pub trait Credential: Send + Sync + std::fmt::Debug {
    /// The auth kind that produced this credential — useful for logging.
    fn kind(&self) -> AuthKind;
}

/// Factory trait for credentials. One impl per auth method.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Classification of this provider — used for display and logging.
    fn kind(&self) -> AuthKind;
    /// Short human-readable name for this provider instance.
    fn display_name(&self) -> &str;
    /// Produce a fresh credential. Refresh semantics are implementation-defined.
    async fn credential(&self) -> crate::Result<Arc<dyn Credential>>;
    /// Whether this provider can be used to access the given resource kind.
    fn supports(&self, resource: ResourceKind) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[derive(Debug)]
    struct FakeCredential;
    impl Credential for FakeCredential {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
    }

    struct FakeProvider;
    #[async_trait]
    impl AuthProvider for FakeProvider {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
        fn display_name(&self) -> &str {
            "fake"
        }
        async fn credential(&self) -> Result<Arc<dyn Credential>, Error> {
            Ok(Arc::new(FakeCredential))
        }
        fn supports(&self, resource: ResourceKind) -> bool {
            matches!(
                resource,
                ResourceKind::BlobContainer | ResourceKind::AdlsContainer
            )
        }
    }

    #[tokio::test]
    async fn trait_object_works() {
        let p: Box<dyn AuthProvider> = Box::new(FakeProvider);
        assert_eq!(p.kind(), AuthKind::Anonymous);
        assert_eq!(p.display_name(), "fake");
        assert!(p.supports(ResourceKind::BlobContainer));
        assert!(!p.supports(ResourceKind::Queue));
        let cred = p.credential().await.unwrap();
        assert_eq!(cred.kind(), AuthKind::Anonymous);
    }
}
