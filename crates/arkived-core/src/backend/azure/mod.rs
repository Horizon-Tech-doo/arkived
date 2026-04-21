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
use crate::backend::types::{
    BlobEntry, BlobPath, ByteStream, Container, DeleteOpts, Page, Range, WriteOpts, WriteResult,
};
use crate::backend::StorageBackend;
use crate::Ctx;
use async_trait::async_trait;
use std::sync::Arc;

/// Azure Blob / ADLS Gen2 backend.
///
/// Construct via [`AzureBlobBackend::new`]. Methods map 1:1 to Azure REST
/// verbs (`list_containers`, `list_blobs`, `read_blob`, `write_blob`,
/// `delete_blob`).
#[derive(Clone)]
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

    /// Construct from a storage account name + Azure environment.
    ///
    /// Builds the endpoint URL as `https://<account>.blob.<env_suffix>`.
    pub fn for_account(
        account_name: &str,
        environment: &crate::types::AzureEnvironment,
        credential: ResolvedCredential,
    ) -> crate::Result<Self> {
        let endpoint = url::Url::parse(&format!(
            "https://{}.blob.{}",
            account_name,
            environment.storage_suffix()
        ))
        .map_err(|e| crate::Error::Backend(format!("build endpoint: {e}")))?;
        Self::new(endpoint, credential)
    }
}

#[async_trait]
impl StorageBackend for AzureBlobBackend {
    fn name(&self) -> &'static str {
        "azure-blob"
    }

    async fn list_containers(
        &self,
        _ctx: &Ctx,
        continuation: Option<String>,
    ) -> crate::Result<Page<Container>> {
        AzureBlobBackend::list_containers(self, continuation).await
    }

    async fn list_blobs(
        &self,
        _ctx: &Ctx,
        container: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        continuation: Option<String>,
    ) -> crate::Result<Page<BlobEntry>> {
        AzureBlobBackend::list_blobs(self, container, prefix, delimiter, continuation).await
    }

    async fn read_blob(
        &self,
        _ctx: &Ctx,
        path: &BlobPath,
        range: Option<Range>,
    ) -> crate::Result<ByteStream> {
        AzureBlobBackend::read_blob(self, path, range).await
    }

    async fn write_blob(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        body: ByteStream,
        opts: WriteOpts,
    ) -> crate::Result<WriteResult> {
        AzureBlobBackend::write_blob(self, ctx, path, body, opts).await
    }

    async fn delete_blob(&self, ctx: &Ctx, path: &BlobPath, opts: DeleteOpts) -> crate::Result<()> {
        AzureBlobBackend::delete_blob(self, ctx, path, opts).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn for_account_builds_public_endpoint() {
        use crate::types::AzureEnvironment;
        let b = AzureBlobBackend::for_account(
            "acmeprod",
            &AzureEnvironment::Public,
            ResolvedCredential::Anonymous,
        )
        .unwrap();
        assert_eq!(
            b.endpoint().as_str(),
            "https://acmeprod.blob.core.windows.net/"
        );
    }

    #[test]
    fn for_account_builds_china_endpoint() {
        use crate::types::AzureEnvironment;
        let b = AzureBlobBackend::for_account(
            "acmeprod",
            &AzureEnvironment::China,
            ResolvedCredential::Anonymous,
        )
        .unwrap();
        assert_eq!(
            b.endpoint().as_str(),
            "https://acmeprod.blob.core.chinacloudapi.cn/"
        );
    }
}
