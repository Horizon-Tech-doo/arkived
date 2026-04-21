//! Anonymous auth — no credential. Works only for publicly-readable resources.

use crate::auth::{AuthProvider, ResolvedCredential};
use crate::types::{AuthKind, ResourceKind};
use async_trait::async_trait;

/// `AuthProvider` that yields `ResolvedCredential::Anonymous`.
#[derive(Debug, Clone, Default)]
pub struct AnonymousProvider;

impl AnonymousProvider {
    /// Construct a new `AnonymousProvider`.
    pub fn new() -> Self { Self }
}

#[async_trait]
impl AuthProvider for AnonymousProvider {
    fn kind(&self) -> AuthKind { AuthKind::Anonymous }
    fn display_name(&self) -> &str { "anonymous" }
    async fn resolve(&self) -> crate::Result<ResolvedCredential> {
        Ok(ResolvedCredential::Anonymous)
    }
    fn supports(&self, resource: ResourceKind) -> bool {
        matches!(resource, ResourceKind::BlobContainer | ResourceKind::AdlsContainer | ResourceKind::AdlsDirectory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolves_anonymous() {
        let p = AnonymousProvider::new();
        assert_eq!(p.kind(), AuthKind::Anonymous);
        assert_eq!(p.display_name(), "anonymous");
        let r = p.resolve().await.unwrap();
        assert!(matches!(r, ResolvedCredential::Anonymous));
    }

    #[test]
    fn supports_only_public_readables() {
        let p = AnonymousProvider::new();
        assert!(p.supports(ResourceKind::BlobContainer));
        assert!(p.supports(ResourceKind::AdlsContainer));
        assert!(!p.supports(ResourceKind::Queue));
        assert!(!p.supports(ResourceKind::Table));
        assert!(!p.supports(ResourceKind::FileShare));
        assert!(!p.supports(ResourceKind::StorageAccount));
    }
}
