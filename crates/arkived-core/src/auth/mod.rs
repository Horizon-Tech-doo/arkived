//! `AuthProvider` trait + credential storage abstractions.

pub mod anonymous;
pub mod azurite;
pub mod credentials;
pub mod resolved;

pub use resolved::ResolvedCredential;

use crate::types::{AuthKind, ResourceKind};
use async_trait::async_trait;

/// Factory trait for auth methods. One impl per auth method.
///
/// `resolve()` produces a [`ResolvedCredential`] — the form the storage
/// backend will consume. Concrete impls land in sibling modules:
/// `anonymous`, `azurite`, `account_key`, `sas`, `connection_string`,
/// `entra`.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Classification of this provider — for logging/UI.
    fn kind(&self) -> AuthKind;
    /// Short human-readable name for this provider instance.
    fn display_name(&self) -> &str;
    /// Materialize the credential. Refresh semantics are impl-defined.
    async fn resolve(&self) -> crate::Result<ResolvedCredential>;
    /// Whether this provider can access the given resource kind.
    fn supports(&self, resource: ResourceKind) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysAnonymousProvider;

    #[async_trait]
    impl AuthProvider for AlwaysAnonymousProvider {
        fn kind(&self) -> AuthKind { AuthKind::Anonymous }
        fn display_name(&self) -> &str { "test-anonymous" }
        async fn resolve(&self) -> crate::Result<ResolvedCredential> {
            Ok(ResolvedCredential::Anonymous)
        }
        fn supports(&self, _: ResourceKind) -> bool { true }
    }

    #[tokio::test]
    async fn trait_object_resolves() {
        let p: Box<dyn AuthProvider> = Box::new(AlwaysAnonymousProvider);
        assert_eq!(p.kind(), AuthKind::Anonymous);
        let r = p.resolve().await.unwrap();
        assert!(matches!(r, ResolvedCredential::Anonymous));
    }
}
