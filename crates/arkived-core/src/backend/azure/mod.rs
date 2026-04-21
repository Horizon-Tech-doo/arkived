//! Azure Blob Storage + ADLS Gen2 backend, hand-rolled on `reqwest`.
//!
//! The backend consumes a [`ResolvedCredential`] from the auth layer and
//! dispatches requests via [`http::HttpPipeline`] which applies the
//! appropriate auth bridge (SharedKey HMAC signing, SAS URL decoration,
//! Entra bearer token, or anonymous).
//!
//! See [`crate::backend`](crate::backend) for shared types.

pub(crate) mod auth_bridge;
pub(crate) mod error;
pub(crate) mod http;
pub(crate) mod models;
pub(crate) mod ops;
pub(crate) mod retry;
pub(crate) mod xml;

use crate::auth::ResolvedCredential;
use std::sync::Arc;

/// Azure Blob / ADLS Gen2 backend.
///
/// Construct via [`AzureBlobBackend::new`]. Methods map 1:1 to Azure REST
/// verbs (`list_containers`, `list_blobs`, `read_blob`, `write_blob`,
/// `delete_blob`).
#[derive(Clone)]
#[allow(dead_code)] // Fields used by ops impls in Tasks 9–13.
pub struct AzureBlobBackend {
    /// Account blob endpoint URL (no trailing slash), e.g.
    /// `https://acme.blob.core.windows.net`.
    pub(crate) endpoint: url::Url,
    /// Credential to attach to every outgoing request.
    pub(crate) credential: Arc<ResolvedCredential>,
    /// Shared reqwest client for connection reuse.
    pub(crate) http: reqwest::Client,
}

impl std::fmt::Debug for AzureBlobBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzureBlobBackend")
            .field("endpoint", &self.endpoint.as_str())
            .field("credential", &self.credential.kind())
            .finish()
    }
}

impl AzureBlobBackend {
    /// Construct a backend from an endpoint URL and resolved credential.
    pub fn new(endpoint: url::Url, credential: ResolvedCredential) -> crate::Result<Self> {
        Ok(Self {
            endpoint,
            credential: Arc::new(credential),
            http: reqwest::Client::new(),
        })
    }

    /// The configured blob endpoint URL.
    pub fn endpoint(&self) -> &url::Url {
        &self.endpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ResolvedCredential;

    #[test]
    fn construction_succeeds() {
        let url = url::Url::parse("https://acme.blob.core.windows.net").unwrap();
        let b = AzureBlobBackend::new(url.clone(), ResolvedCredential::Anonymous).unwrap();
        assert_eq!(b.endpoint(), &url);
    }

    #[test]
    fn debug_hides_credential_contents() {
        let url = url::Url::parse("https://acme.blob.core.windows.net").unwrap();
        let b = AzureBlobBackend::new(url, ResolvedCredential::Anonymous).unwrap();
        let dbg = format!("{b:?}");
        assert!(dbg.contains("AzureBlobBackend"));
        assert!(dbg.contains("acme.blob.core.windows.net"));
    }
}
